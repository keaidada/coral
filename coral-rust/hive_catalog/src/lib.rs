// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Hive Metastore catalog client for [`coral_core`].
//!
//! Implements [`coral_core::Catalog`] against a remote Hive Metastore / Iceberg
//! REST / Unity Catalog-style endpoint. The specific transport is pluggable
//! via the [`Transport`] trait so you can drop in `reqwest` (async),
//! `hyper`, or a thrift client without touching the catalog logic itself.
//!
//! # Fetch-on-first-use + cache
//!
//! Column lookups are NOT free over the network. This crate caches per
//! `(db, table)` pair after the first fetch.
//!
//! ## Why the cache leaks
//!
//! `coral_core::Catalog::columns_of` returns `Option<&[String]>` — a borrow
//! whose lifetime is tied to `&self`. To hand out such a borrow from behind
//! a `Mutex<HashMap<...>>`, we'd need self-referential magic or a RwLock
//! with scoped guards (pre-NLL2024 that API is awkward).
//!
//! Simpler approach: on first fetch, we `Box::leak` the column vector. The
//! returned `&'static [String]` outlives everything, so we can stash it in
//! a separate `HashMap<_, &'static [String]>` and hand it back under any
//! lifetime. Net cost: ~N * 32 bytes per unique table queried during the
//! process lifetime. For translation workloads that's measured in KB.
//!
//! Long-lived services with very high cardinality of (db, table) pairs
//! should call [`HiveCatalog::invalidate_all`] periodically — invalidation
//! drops the stashed pointer, letting the underlying String allocations
//! eventually be freed (they won't be: leaked means leaked). If that's a
//! problem, the upstream `Catalog` trait will need an owned-return variant
//! before we can be smarter here.
//!
//! # Minimal usage
//!
//! ```no_run
//! use coral_hive_catalog::{HiveCatalog, HttpTransport};
//!
//! let transport = HttpTransport::new("http://metastore.internal:9083/api");
//! let catalog = HiveCatalog::new(transport);
//!
//! let r = coral_core::translate_with_catalog(
//!     "SELECT e.id, e.name FROM analytics.employees e",
//!     &catalog,
//! ).unwrap();
//! # let _ = r;
//! ```

use std::collections::HashMap;
use std::sync::Mutex;

use coral_core::Catalog;
use serde::Deserialize;
use thiserror::Error;

/// Errors that can occur when talking to a remote metastore.
#[derive(Debug, Error)]
pub enum CatalogError {
    /// Network / transport failure (timeout, DNS, TLS, HTTP non-2xx).
    #[error("transport error: {0}")]
    Transport(String),

    /// Server responded but the JSON didn't match our schema.
    #[error("response parse error: {0}")]
    BadResponse(String),
}

/// Trait describing how to reach the metastore. The default implementation
/// is [`HttpTransport`] (blocking HTTP via `ureq`). Projects that need async
/// should supply their own impl that wraps `reqwest` or similar.
pub trait Transport: Send + Sync {
    /// Fetch the JSON-encoded schema for a `db.table`. Implementations should
    /// return `Ok(None)` when the server replies 404 / "table not found",
    /// and `Err(...)` for any transport-level failure.
    fn get_table_schema(
        &self,
        db: &str,
        table: &str,
    ) -> Result<Option<TableSchemaJson>, CatalogError>;
}

/// Wire format shared across Hive Metastore REST and Iceberg/Unity Catalog
/// variants we've seen in the wild. Mapping per source:
///
/// | Source                 | Endpoint                                   |
/// |------------------------|--------------------------------------------|
/// | Hive Metastore REST    | `GET /api/databases/{db}/tables/{table}`   |
/// | Iceberg REST           | `GET /v1/namespaces/{db}/tables/{table}`   |
/// | Unity Catalog          | `GET /api/2.1/unity-catalog/tables/...`    |
///
/// [`HttpTransport`] targets the first form; other transports can adapt.
#[derive(Debug, Clone, Deserialize)]
pub struct TableSchemaJson {
    pub columns: Vec<ColumnJson>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ColumnJson {
    pub name: String,
    /// Hive-style type string, e.g. `string`, `bigint`, `decimal(10,2)`.
    #[serde(alias = "type_text")]
    pub r#type: String,
}

/// Default transport: blocking HTTP via `ureq`. Suitable for scripts,
/// CLIs, lambda functions, and anything that isn't inside an async runtime.
pub struct HttpTransport {
    base_url: String,
    agent: ureq::Agent,
}

impl HttpTransport {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into().trim_end_matches('/').to_string(),
            agent: ureq::AgentBuilder::new().build(),
        }
    }

    /// Override the base URL later (e.g. in tests, pointing at a mock server).
    pub fn with_base_url(mut self, base: impl Into<String>) -> Self {
        self.base_url = base.into().trim_end_matches('/').to_string();
        self
    }
}

impl Transport for HttpTransport {
    fn get_table_schema(
        &self,
        db: &str,
        table: &str,
    ) -> Result<Option<TableSchemaJson>, CatalogError> {
        // URL-encode db/table to avoid injection via quoted identifiers.
        let url = format!(
            "{}/databases/{}/tables/{}",
            self.base_url,
            url_segment(db),
            url_segment(table)
        );
        match self.agent.get(&url).call() {
            Ok(resp) => {
                let schema: TableSchemaJson = resp
                    .into_json()
                    .map_err(|e| CatalogError::BadResponse(e.to_string()))?;
                Ok(Some(schema))
            }
            Err(ureq::Error::Status(404, _)) => Ok(None),
            Err(e) => Err(CatalogError::Transport(e.to_string())),
        }
    }
}

/// Bare-bones URL segment encoder: alphanumeric + `_ - .` pass through;
/// everything else is percent-encoded. Good enough for SQL identifiers;
/// NOT a general-purpose encoder.
fn url_segment(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'-' | b'.' => out.push(b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

type CacheKey = (String, String);

/// Catalog implementation backed by a `Transport`. Caches schemas per
/// `(db, table)` so hot queries only hit the network on the first reference.
pub struct HiveCatalog<T: Transport> {
    transport: T,
    /// Holds the leaked `&'static [String]` slices we hand out of
    /// `columns_of`. See the crate-level docs for why the leak is OK.
    cache: Mutex<HashMap<CacheKey, Option<&'static [String]>>>,
}

impl<T: Transport> HiveCatalog<T> {
    pub fn new(transport: T) -> Self {
        Self {
            transport,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Evict a single cached entry (e.g. after a CREATE / ALTER that happens
    /// out-of-band). Does not free the leaked columns memory.
    pub fn invalidate(&self, db: &str, table: &str) {
        let key = (db.to_ascii_lowercase(), table.to_ascii_lowercase());
        self.cache.lock().unwrap().remove(&key);
    }

    /// Drop every cached entry. Does not free the leaked columns memory.
    pub fn invalidate_all(&self) {
        self.cache.lock().unwrap().clear();
    }

    /// Borrow access to the underlying transport. Handy for tests and for
    /// adding retries / metrics one level up.
    pub fn transport(&self) -> &T {
        &self.transport
    }

    /// Pre-warm the cache. Useful when you know ahead of time which tables
    /// a query touches (e.g. from a query planner).
    pub fn prefetch(&self, db: &str, table: &str) {
        let _ = self.fetch_and_cache(db, table);
    }

    /// Internal: fetch from transport, leak, stash, return the leaked slice.
    fn fetch_and_cache(&self, db: &str, table: &str) -> Option<&'static [String]> {
        let key = (db.to_ascii_lowercase(), table.to_ascii_lowercase());

        // Hot path: cache hit (positive or negative).
        {
            let guard = self.cache.lock().unwrap();
            if let Some(entry) = guard.get(&key) {
                return *entry;
            }
        }

        // Cold path: fetch + leak + stash.
        let result: Option<&'static [String]> = match self.transport.get_table_schema(db, table) {
            Ok(Some(schema)) => {
                let cols: Vec<String> = schema.columns.into_iter().map(|c| c.name).collect();
                let leaked: &'static [String] = Box::leak(cols.into_boxed_slice());
                Some(leaked)
            }
            Ok(None) => None, // negative cache entry
            Err(_) => {
                // Transport error — DO NOT cache so the next call retries.
                return None;
            }
        };

        let mut guard = self.cache.lock().unwrap();
        // It's fine if another thread beat us to the insert — they leaked too,
        // but both slices have identical contents and both live forever.
        guard.insert(key, result);
        result
    }
}

impl<T: Transport> Catalog for HiveCatalog<T> {
    fn columns_of(&self, db: &str, table: &str) -> Option<&[String]> {
        // The leaked slice is `&'static`, so lifetime-shortening to `&'a` for
        // the trait return type is trivially sound.
        self.fetch_and_cache(db, table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Test transport that counts how many times get_table_schema is called
    /// so we can assert the cache behavior.
    struct CountingTransport {
        tables: HashMap<CacheKey, Vec<String>>,
        calls: AtomicUsize,
    }

    impl CountingTransport {
        fn new() -> Self {
            let mut t = HashMap::new();
            t.insert(
                ("default".into(), "employees".into()),
                vec!["id".into(), "name".into(), "dept_id".into()],
            );
            Self {
                tables: t,
                calls: AtomicUsize::new(0),
            }
        }
    }

    impl Transport for CountingTransport {
        fn get_table_schema(
            &self,
            db: &str,
            table: &str,
        ) -> Result<Option<TableSchemaJson>, CatalogError> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let key = (db.to_ascii_lowercase(), table.to_ascii_lowercase());
            Ok(self.tables.get(&key).map(|cols| TableSchemaJson {
                columns: cols
                    .iter()
                    .map(|c| ColumnJson {
                        name: c.clone(),
                        r#type: "string".into(),
                    })
                    .collect(),
            }))
        }
    }

    #[test]
    fn fetch_and_cache_hit() {
        let t = CountingTransport::new();
        let cat = HiveCatalog::new(t);

        let cols1 = cat.columns_of("default", "employees").unwrap().to_vec();
        assert_eq!(cols1, vec!["id", "name", "dept_id"]);

        // Second call must hit the cache, not the transport.
        let _cols2 = cat.columns_of("default", "employees").unwrap();
        let calls = cat.transport().calls.load(Ordering::SeqCst);
        assert_eq!(calls, 1, "expected 1 transport call, got {calls}");
    }

    #[test]
    fn missing_table_is_negatively_cached() {
        let t = CountingTransport::new();
        let cat = HiveCatalog::new(t);

        assert!(cat.columns_of("default", "nonesuch").is_none());
        assert!(cat.columns_of("default", "nonesuch").is_none());
        assert_eq!(cat.transport().calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn invalidate_forces_refetch() {
        let t = CountingTransport::new();
        let cat = HiveCatalog::new(t);

        let _ = cat.columns_of("default", "employees");
        cat.invalidate("default", "employees");
        let _ = cat.columns_of("default", "employees");
        assert_eq!(cat.transport().calls.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn case_insensitive_cache_key() {
        let t = CountingTransport::new();
        let cat = HiveCatalog::new(t);

        let _ = cat.columns_of("DEFAULT", "Employees");
        let _ = cat.columns_of("default", "employees");
        // Both spellings hash to the same key -> exactly one transport call.
        assert_eq!(cat.transport().calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn url_segment_encodes_unsafe() {
        assert_eq!(url_segment("foo"), "foo");
        assert_eq!(url_segment("foo bar"), "foo%20bar");
        assert_eq!(url_segment("a'b"), "a%27b");
    }
}
