// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Public translation entry points.
//!
//! Mirrors `CoralGaussDBToSpark.createLocal(...).getSparkSql()` from the Java tree,
//! minus the catalog resolution step (the Rust port is pure text->text; we don't
//! need schemas for translation-only use cases).

use sqlparser::ast::Statement;
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

use crate::catalog::{validate_against, Catalog, ValidationIssue};
use crate::error::Result;
use crate::preprocess::preprocess;
use crate::rewrite::apply_all;

/// Translate a single GaussDB / openGauss SQL statement to Spark SQL.
///
/// Pipeline:
///
/// 1. **Text-level preprocessing** — rewrites Oracle-style
///    `START WITH ... CONNECT BY` into `WITH RECURSIVE`, and Oracle `(+)`
///    outer-join markers into standard `LEFT JOIN`. Neither is supported by
///    `sqlparser-rs`'s PostgreSQL dialect directly.
/// 2. **Parse** with `sqlparser-rs`'s PostgreSQL dialect (GaussDB is
///    PostgreSQL-compatible).
/// 3. **AST rewrites** — every pass in [`rewrite::apply_all`] (DISTINCT ON,
///    function registry, type mapping).
/// 4. **Display** — emit via `Statement`'s `Display` impl.
pub fn translate(gaussdb_sql: &str) -> Result<String> {
    let preprocessed = preprocess(gaussdb_sql);
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, &preprocessed)?;

    if statements.is_empty() {
        return Ok(String::new());
    }

    for stmt in statements.iter_mut() {
        apply_all(stmt);
    }

    Ok(render(&statements))
}

/// Translate multiple statements (separated by `;`) in one pass.
///
/// Returns a Vec of the translated statements preserving input order.
pub fn translate_all(gaussdb_sql: &str) -> Result<Vec<String>> {
    let preprocessed = preprocess(gaussdb_sql);
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, &preprocessed)?;

    for stmt in statements.iter_mut() {
        apply_all(stmt);
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
    /// Translated Spark SQL (same as what [`translate`] would return).
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
    let preprocessed = preprocess(gaussdb_sql);
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, &preprocessed)?;

    let mut issues = vec![];
    for stmt in statements.iter_mut() {
        // Validate BEFORE rewriting so error messages refer to the user's
        // original table/column names.
        issues.extend(validate_against(stmt, catalog));
        apply_all(stmt);
    }

    Ok(CatalogTranslation {
        spark_sql: render(&statements),
        issues,
    })
}
