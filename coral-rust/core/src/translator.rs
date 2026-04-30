// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Public translation entry points.
//!
//! Mirrors `CoralGaussDBToSpark.createLocal(...).getSparkSql()` and
//! `HiveToTrinoConverter.toTrinoSql(...)` from the Java tree, minus the
//! catalog resolution step (the Rust port is pure text->text; we don't
//! need schemas for translation-only use cases).

use sqlparser::ast::{Expr, Function, Statement, Visit, Visitor};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::BTreeSet;
use std::ops::ControlFlow;

use crate::catalog::{validate_against, Catalog, ValidationIssue};
use crate::error::Result;
use crate::format::{
    add_default_aliases, drop_as_in_from, pretty_print, qualify_with_default_db,
    quote_idents_in_from,
};
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
    translate_to_with(sql, target, /* pretty */ false)
}

/// Return function names that are not present in Coral's known function registry.
pub fn unknown_functions(sql: &str) -> Result<Vec<String>> {
    let preprocessed = preprocess(sql);
    let dialect = PostgreSqlDialect {};
    let statements = Parser::parse_sql(&dialect, &preprocessed)?;
    let mut collector = UnknownFunctionCollector {
        names: BTreeSet::new(),
    };
    let _ = statements.visit(&mut collector);
    Ok(collector.names.into_iter().collect())
}

/// Target-aware translator with an explicit pretty-print switch.
///
/// When `pretty = true` the renderer mirrors Java Coral's Calcite
/// pretty-printer: prepends `default.` to bare table names,
/// auto-aliases every `FROM table`, breaks top-level SELECT clauses
/// onto their own lines, and (for Trino) wraps identifiers in double
/// quotes. Spark output omits the `AS` keyword between a table and
/// its alias (`FROM default.t t`), matching Java's Spark dialect.
/// When `pretty = false` (the default for `translate()` /
/// `translate_to()`) the output stays compact — this preserves the
/// golden test suite and is friendlier for programmatic consumers.
pub fn translate_to_with(sql: &str, target: Target, pretty: bool) -> Result<String> {
    let preprocessed = preprocess(sql);
    let dialect = PostgreSqlDialect {};
    let mut statements = Parser::parse_sql(&dialect, &preprocessed)?;

    if statements.is_empty() {
        return Ok(String::new());
    }

    for stmt in statements.iter_mut() {
        apply_all_for_target(stmt, target);
        if pretty {
            qualify_with_default_db(stmt, "default");
            add_default_aliases(stmt);
        }
    }

    let raw = render_with(&statements, pretty);
    if !pretty {
        return Ok(raw);
    }

    // Per-target final formatting. Java's output:
    //   Spark: FROM default.users users        (no AS, no quotes)
    //   Trino: FROM "default"."users" AS "users"  (quotes + AS)
    let final_text = match target {
        Target::Spark => drop_as_in_from(&raw),
        Target::Trino => quote_idents_in_from(&raw),
    };
    Ok(final_text)
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
    render_with(statements, /* pretty */ false)
}

struct UnknownFunctionCollector {
    names: BTreeSet<String>,
}

impl Visitor for UnknownFunctionCollector {
    type Break = std::convert::Infallible;

    fn post_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        if let Expr::Function(f) = expr {
            let name = function_name_lower(f);
            if !name.is_empty() && crate::function_catalog::lookup(&name).is_none() {
                self.names.insert(name);
            }
        }
        ControlFlow::Continue(())
    }
}

fn function_name_lower(f: &Function) -> String {
    f.name
        .0
        .last()
        .map(|i| i.value.to_lowercase())
        .unwrap_or_default()
}

/// Shared renderer used by both the compact and pretty-printed paths.
/// `pretty = false` matches sqlparser's default `Display` output and
/// keeps golden tests stable. `pretty = true` runs every statement
/// through [`format::pretty_print`] to land on Calcite-style clause
/// boundaries (what Java Coral emits).
fn render_with(statements: &[Statement], pretty: bool) -> String {
    statements
        .iter()
        .map(|s| {
            let raw = s.to_string();
            if pretty {
                pretty_print(&raw)
            } else {
                raw
            }
        })
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
