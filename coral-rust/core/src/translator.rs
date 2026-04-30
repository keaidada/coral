// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Public translation entry points.
//!
//! Mirrors `CoralGaussDBToSpark.createLocal(...).getSparkSql()` and
//! `HiveToTrinoConverter.toTrinoSql(...)` from the Java tree, minus the
//! catalog resolution step (the Rust port is pure text->text; we don't
//! need schemas for translation-only use cases).

use sqlparser::ast::Statement;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

use crate::catalog::{validate_against, Catalog, ValidationIssue};
use crate::error::Result;
use crate::preprocess::preprocess;
use crate::rewrite::apply_all_for_target;
use crate::target::Target;

/// Translate a single GaussDB / openGauss SQL statement to Spark SQL.
///
/// Pipeline:
///
/// 1. **Text-level preprocessing** — rewrites Oracle-style
///    `START WITH ... CONNECT BY` into `WITH RECURSIVE`, and Oracle `(+)`
///    outer-join markers into standard `LEFT JOIN`. Neither is supported
///    by `sqlparser-rs`'s PostgreSQL dialect directly.
/// 2. **Parse** with `sqlparser-rs`'s PostgreSQL dialect (GaussDB is
///    PostgreSQL-compatible).
/// 3. **AST rewrites** — every pass in [`rewrite::apply_all`] (DISTINCT ON,
///    function registry, type mapping).
/// 4. **Display** — emit via `Statement`'s `Display` impl.
pub fn translate(gaussdb_sql: &str) -> Result<String> {
    translate_to(gaussdb_sql, Target::Spark)
}

/// Translate directly to Trino SQL.
///
/// Same pipeline as [`translate`], but with the Trino-specific function and
/// type-rewrite passes layered on top of the Spark-compatible rewrites.
/// This mirrors Java `coral-trino`'s `HiveToTrinoConverter.toTrinoSql`.
pub fn translate_to_trino(gaussdb_sql: &str) -> Result<String> {
    translate_to(gaussdb_sql, Target::Trino)
}

/// Target-aware translation entry point used by both [`translate`] and
/// [`translate_to_trino`].
pub fn translate_to(sql: &str, target: Target) -> Result<String> {
    let preprocessed = preprocess(sql);
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, &preprocessed)?;

    if statements.is_empty() {
        return Ok(String::new());
    }

    for stmt in statements.iter_mut() {
        apply_all_for_target(stmt, target);
    }

    Ok(render(&statements))
}

/// Translate multiple statements (separated by `;`) in one pass to Spark SQL.
///
/// Returns a Vec of the translated statements preserving input order.
pub fn translate_all(gaussdb_sql: &str) -> Result<Vec<String>> {
    translate_all_to(gaussdb_sql, Target::Spark)
}

/// Like [`translate_all`] but target-aware.
pub fn translate_all_to(sql: &str, target: Target) -> Result<Vec<String>> {
    let preprocessed = preprocess(sql);
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, &preprocessed)?;

    for stmt in statements.iter_mut() {
        apply_all_for_target(stmt, target);
    }

    Ok(statements.iter().map(|s| s.to_string()).collect())
}

fn render(statements: &[Statement]) -> String {
    statements
        .iter()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
        .join(";\n")
}

/// Result of a catalog-aware translation.
///
/// See [`translate_with_catalog`] for usage.
#[derive(Debug, Clone)]
pub struct CatalogTranslation {
    /// Translated SQL (Spark by default; Trino when using
    /// [`translate_with_catalog_to`]).
    pub spark_sql: String,
    /// Soft validation warnings from the catalog. Translation always
    /// proceeds regardless; callers decide whether to log or escalate.
    pub issues: Vec<ValidationIssue>,
}

/// Like [`translate`] but also validates references against `catalog`.
///
/// The returned [`CatalogTranslation`] holds both the Spark SQL and a list of
/// soft [`ValidationIssue`]s (unknown tables / columns, with "did you mean?"
/// suggestions). Issues do NOT fail translation — they are intended for
/// logging or linting.
pub fn translate_with_catalog<C: Catalog>(
    gaussdb_sql: &str,
    catalog: &C,
) -> Result<CatalogTranslation> {
    translate_with_catalog_to(gaussdb_sql, catalog, Target::Spark)
}

/// Target-aware catalog-validated translation.
pub fn translate_with_catalog_to<C: Catalog>(
    sql: &str,
    catalog: &C,
    target: Target,
) -> Result<CatalogTranslation> {
    let preprocessed = preprocess(sql);
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, &preprocessed)?;

    let mut issues = vec![];
    for stmt in statements.iter_mut() {
        // Validate BEFORE rewriting so error messages refer to the user's
        // original table/column names.
        issues.extend(validate_against(stmt, catalog));
        apply_all_for_target(stmt, target);
    }

    Ok(CatalogTranslation {
        spark_sql: render(&statements),
        issues,
    })
}