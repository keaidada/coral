// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! AST rewrite passes that lower GaussDB / openGauss SQL to Spark SQL.
//!
//! The Java `coral-gaussdb` tree does this work at two layers:
//!   1. ParseTreeBuilder (AST visitor) — turns GaussDB-specific syntax into
//!      dialect-neutral Calcite SqlNodes.
//!   2. Coral rewrite rules — restructure SqlNodes (e.g. DISTINCT ON ->
//!      ROW_NUMBER()).
//!
//! The Rust port collapses both into a single family of AST passes running
//! directly against `sqlparser-rs` nodes. Each pass implements [`VisitorMut`]
//! and is applied in a fixed order by [`apply_all`].

use sqlparser::ast::{Statement, VisitMut};

pub mod functions;
pub mod structure;

/// Run every rewrite pass against a single statement, in the order:
///
/// 1. [`structure::DistinctOnRewriter`] — rewrites `DISTINCT ON (...)` into
///    `SELECT ... FROM (SELECT ..., ROW_NUMBER() OVER (...) rn) WHERE rn = 1`.
///    Runs **before** function rewrites because it restructures whole
///    `Select` bodies and we want functions inside the newly-wrapped
///    subquery still to go through function-level rewriting.
///
/// 2. [`functions::FunctionRewriter`] — rewrites GaussDB-specific functions
///    and operators (NVL → COALESCE, DECODE → CASE, SUBSTR → SUBSTRING,
///    MOD → %, ~ → RLIKE, ~* → LOWER(RLIKE LOWER)).
pub fn apply_all(stmt: &mut Statement) {
    let _ = stmt.visit(&mut structure::DistinctOnRewriter);
    let _ = stmt.visit(&mut functions::FunctionRewriter);
}
