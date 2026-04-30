// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Incremental materialized-view rewriter.
//!
//! Rust port of Java `coral-incremental`'s
//! `RelNodeIncrementalTransformer.convertRelIncremental`. Given a SQL
//! query and a naming convention (default: append `_delta`), rewrite
//! the query so it reads from **delta tables** instead of base tables,
//! and emit enough variants to cover the "incremental" semantics:
//!
//!   For a query `Q` that references N tables, the incremental form is
//!   `UNION ALL` over every non-empty subset of those N tables where
//!   the subset is the set of tables replaced by their delta variant.
//!   Taking the union of all `(2^N - 1)` non-trivial subsets gives
//!   you the "fresh rows" that need to be added when any base table
//!   has changed.
//!
//! # Example
//!
//! Input:
//! ```sql
//! SELECT a.id, b.name FROM a JOIN b ON a.k = b.k
//! ```
//!
//! Output (default suffix `_delta`):
//! ```sql
//! SELECT a.id, b.name FROM a_delta JOIN b      ON a.k = b.k
//! UNION ALL
//! SELECT a.id, b.name FROM a       JOIN b_delta ON a.k = b.k
//! UNION ALL
//! SELECT a.id, b.name FROM a_delta JOIN b_delta ON a.k = b.k
//! ```
//!
//! # Rules
//!
//! - One base-table reference → one rewrite (the `_delta` form).
//! - N base-table references → `2^N - 1` rewrites, union'd with
//!   `UNION ALL`.
//! - Filters / projections / aggregates / set operations propagate
//!   through untouched (table refs inside subqueries also get the
//!   delta treatment, which matches the Java semantics).
//! - We stop at the first 4 base tables (`MAX_TABLES`) to keep the
//!   output size bounded — the Java tree has the same guard.

pub mod error;
pub mod rewrite;

pub use error::{IncrementalError, Result};
pub use rewrite::{incremental_sql, incremental_sql_with};

/// Cap the number of base-table references we'll expand combinatorially.
/// 4 tables → 15 UNION ALL branches, already unwieldy; any more and you
/// probably want a real CDC approach instead.
pub const MAX_TABLES: usize = 4;
