// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Trino SQL output backend for coral-rust.
//!
//! Rust port of Java `coral-trino`'s `HiveToTrinoConverter`. The translation
//! pipeline lives in `coral-core`; this crate is intentionally a thin facade
//! so that downstream users who only need Trino output get a crate that
//! compiles in under a second and doesn't pull any extra dependencies
//! beyond `coral-core` itself.
//!
//! # Example
//!
//! ```
//! use coral_trino::to_trino_sql;
//!
//! let hive = "SELECT NVL(name, 'n/a'), RAND() FROM t";
//! let trino = to_trino_sql(hive).unwrap();
//! assert!(trino.contains("COALESCE"));
//! assert!(trino.contains("RANDOM()"));
//! assert!(!trino.contains("NVL"));
//! ```
//!
//! # What gets rewritten
//!
//! Both the Spark-compatible rewrites (`NVL` → `COALESCE`, `(+)` → `LEFT
//! JOIN`, `DISTINCT ON` → `ROW_NUMBER` subquery, …) AND the Trino-specific
//! diff passes (`RAND` → `RANDOM`, `DATE_ADD` two-arg → three-arg with
//! unit literal, `FLOAT` → `REAL`, …). See
//! `coral_core::rewrite::trino_functions` and
//! `coral_core::rewrite::trino_types` for the authoritative rule list.

pub use coral_core::{translate_to_trino as to_trino_sql, CoralError, Result};

/// Translate a full multi-statement script to Trino SQL, returning each
/// statement as a separate string (preserves input order).
///
/// Convenience wrapper on top of [`coral_core::translate_all_to`].
pub fn to_trino_sql_all(sql: &str) -> Result<Vec<String>> {
    coral_core::translate_all_to(sql, coral_core::Target::Trino)
}
