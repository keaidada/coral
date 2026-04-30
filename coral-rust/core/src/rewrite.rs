// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! AST rewrite passes that lower GaussDB / openGauss SQL to Spark SQL
//! (or Trino SQL, when called via [`apply_all_for_target`]).
//!
//! The Java `coral-gaussdb` tree does this work at two layers:
//!   1. ParseTreeBuilder (AST visitor) — turns GaussDB-specific syntax into
//!      dialect-neutral Calcite SqlNodes.
//!   2. Coral rewrite rules — restructure SqlNodes (e.g. DISTINCT ON ->
//!      ROW_NUMBER()).
//!
//! The Rust port collapses both into a single family of AST passes running
//! directly against `sqlparser-rs` nodes. [`apply_all`] is the Spark path;
//! [`apply_all_for_target`] selects between Spark and Trino output.

use sqlparser::ast::{Statement, VisitMut};

use crate::target::Target;

pub mod functions;
pub mod structure;
pub mod trino_functions;
pub mod trino_types;
pub mod types;

/// Run every rewrite pass against a single statement targeting Spark SQL.
///
/// Order:
///
/// 1. [`structure::DistinctOnRewriter`] — rewrites `DISTINCT ON (...)` into
///    `SELECT ... FROM (SELECT ..., ROW_NUMBER() OVER (...) rn) WHERE rn = 1`.
///    Runs **before** function rewrites because it restructures whole
///    `Select` bodies.
///
/// 2. [`functions::FunctionRewriter`] — rewrites GaussDB-specific functions
///    and operators (NVL → COALESCE, DECODE → CASE, SUBSTR → SUBSTRING,
///    MOD → %, ~ → RLIKE, ~* → LOWER(RLIKE LOWER), …), plus date-format
///    token translation inside TO_CHAR / TO_DATE / TO_TIMESTAMP.
///
/// 3. [`types::TypeRewriter`] — rewrites GaussDB-specific data types
///    (JSON/JSONB/UUID/BYTEA/TIMESTAMPTZ/TEXT) inside CAST expressions and
///    CREATE TABLE columns.
pub fn apply_all(stmt: &mut Statement) {
    apply_all_for_target(stmt, Target::Spark);
}

/// Target-aware dispatcher: runs the Spark pipeline, then layers on the
/// Trino diff passes when `target == Trino`.
pub fn apply_all_for_target(stmt: &mut Statement, target: Target) {
    let _ = stmt.visit(&mut structure::DistinctOnRewriter);
    let _ = stmt.visit(&mut functions::FunctionRewriter);
    let _ = stmt.visit(&mut types::TypeRewriter);

    if target == Target::Trino {
        let _ = stmt.visit(&mut trino_functions::TrinoFunctionRewriter);
        let _ = stmt.visit(&mut trino_types::TrinoTypeRewriter);
    }
}
