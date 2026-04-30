// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Spark view preparation — the session-free subset of Java's
//! `CoralSparkViewCatalog.loadView`.
//!
//! Given a view's DDL and a catalog, emit everything a Spark driver
//! needs to re-register the view:
//!
//!   - the **translated Spark SQL** (fully rewritten through coral-core),
//!   - the **Avro schema** for the view's output (via coral-schema),
//!   - the **referenced table list** (for dependency tracking).
//!
//! The caller is responsible for the SparkSession side of things:
//! `sparkSession.catalog().createTable(...)`, `spark.udf.register(...)`,
//! and so on. This crate intentionally does NOT speak JVM — keeping the
//! compile graph small and the output trivially consumable from Python
//! / Scala / Rust Spark drivers alike.

use coral_core::translate;
use coral_schema::TypedCatalog;
use sqlparser::ast::{Statement, TableFactor, Visit, Visitor};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::collections::BTreeSet;
use std::ops::ControlFlow;

use crate::error::{Result, SparkError};

/// Everything a Spark driver needs to register a translated view.
#[derive(Debug, Clone)]
pub struct SparkView {
    /// View name (last identifier of the CREATE VIEW name).
    pub name: String,
    /// Spark SQL body for the view (translated via coral-core).
    pub spark_sql: String,
    /// Avro schema JSON for the view's output columns.
    pub avro_schema: String,
    /// Physical tables referenced by the view, in `"db.table"` form.
    /// Sorted + deduplicated.
    pub referenced_tables: Vec<String>,
}

/// Convert a `CREATE VIEW ... AS SELECT ...` (or bare SELECT) into a
/// [`SparkView`] using `catalog` for column type resolution.
///
/// This is the session-free portion of Java `CoralSparkViewCatalog`:
///   - parse the DDL
///   - collect referenced `db.table` physical names
///   - translate the SELECT body to Spark SQL via coral-core
///   - derive Avro schema via coral-schema
///
/// Returns a [`SparkView`] the caller passes to their own Spark code.
pub fn prepare_view<C: TypedCatalog>(ddl: &str, catalog: &C) -> Result<SparkView> {
    let dialect = PostgreSqlDialect {};
    let stmts = Parser::parse_sql(&dialect, ddl)?;

    let (name, select_text) = extract_view_parts(&stmts, ddl)?;

    let spark_sql = translate(&select_text)?;
    let avro_schema = coral_schema::to_avro_schema(ddl, catalog)?;

    let mut collector = TableCollector {
        tables: BTreeSet::new(),
    };
    let _ = stmts.visit(&mut collector);

    Ok(SparkView {
        name,
        spark_sql,
        avro_schema,
        referenced_tables: collector.tables.into_iter().collect(),
    })
}

// -------------------------------------------------------------------
// AST walks
// -------------------------------------------------------------------

fn extract_view_parts(stmts: &[Statement], original: &str) -> Result<(String, String)> {
    for stmt in stmts {
        match stmt {
            Statement::CreateView { name, query, .. } => {
                let view_name = name
                    .0
                    .last()
                    .map(|i| i.value.clone())
                    .unwrap_or_else(|| "view".to_string());
                return Ok((view_name, query.to_string()));
            }
            Statement::Query(_) => {
                return Ok(("view".to_string(), original.to_string()));
            }
            _ => continue,
        }
    }
    Err(SparkError::Plan(
        "no CREATE VIEW or SELECT found in input DDL".to_string(),
    ))
}

struct TableCollector {
    tables: BTreeSet<String>,
}

impl Visitor for TableCollector {
    type Break = std::convert::Infallible;

    fn post_visit_table_factor(&mut self, factor: &TableFactor) -> ControlFlow<Self::Break> {
        if let TableFactor::Table { name, .. } = factor {
            let parts: Vec<String> = name.0.iter().map(|i| i.value.clone()).collect();
            let qualified = match parts.len() {
                1 => format!("default.{}", parts[0]),
                2 => format!("{}.{}", parts[0], parts[1]),
                _ => parts.join("."),
            };
            self.tables.insert(qualified);
        }
        ControlFlow::Continue(())
    }
}
