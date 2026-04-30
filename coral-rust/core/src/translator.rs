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
