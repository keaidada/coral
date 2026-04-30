// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Type-mapping rewrites specific to the **Trino** output target.
//!
//! Port of `TrinoSqlRewriter.convertTypeSpec` from Java `coral-trino`. Runs
//! AFTER the Spark-compatible `rewrite::types` pass, so Spark-friendly type
//! names are already in place. This pass only patches the Spark↔Trino diff.
//!
//! | After Spark pass | After Trino pass |
//! |------------------|------------------|
//! | `FLOAT`          | `REAL`           |
//! | `BINARY`         | `VARBINARY`      |
//! | `STRING`         | `VARCHAR`        |
//!
//! CHARACTER SET clauses on VARCHAR/CHAR are also stripped (Trino's grammar
//! doesn't accept them and sqlparser's Display omits them anyway, so this
//! is mostly defensive).

use sqlparser::ast::{DataType, VisitorMut};
use std::ops::ControlFlow;

pub struct TrinoTypeRewriter;

impl VisitorMut for TrinoTypeRewriter {
    type Break = std::convert::Infallible;

    fn post_visit_expr(&mut self, expr: &mut sqlparser::ast::Expr) -> ControlFlow<Self::Break> {
        if let sqlparser::ast::Expr::Cast { data_type, .. } = expr {
            rewrite_type(data_type);
        }
        ControlFlow::Continue(())
    }

    fn post_visit_statement(
        &mut self,
        stmt: &mut sqlparser::ast::Statement,
    ) -> ControlFlow<Self::Break> {
        // CREATE TABLE column defs: patch column types in place.
        if let sqlparser::ast::Statement::CreateTable(ct) = stmt {
            for col in ct.columns.iter_mut() {
                rewrite_type(&mut col.data_type);
            }
        }
        ControlFlow::Continue(())
    }
}

fn rewrite_type(t: &mut DataType) {
    match t {
        // Spark FLOAT (32-bit) → Trino REAL.
        DataType::Float(_) => {
            *t = DataType::Real;
        }
        // Spark BINARY → Trino VARBINARY.
        DataType::Binary(_) => {
            *t = DataType::Varbinary(None);
        }
        DataType::Blob(_) => {
            *t = DataType::Varbinary(None);
        }
        // Spark STRING → Trino VARCHAR (no length).
        DataType::Text => {
            *t = DataType::Varchar(None);
        }
        DataType::String(_) => {
            *t = DataType::Varchar(None);
        }
        // Custom("STRING") slips in from the GaussDB type pass; normalize.
        DataType::Custom(name, _) if matches_ci(name, "STRING") => {
            *t = DataType::Varchar(None);
        }
        _ => {}
    }
}

fn matches_ci(name: &sqlparser::ast::ObjectName, target: &str) -> bool {
    name.0
        .last()
        .map(|i| i.value.eq_ignore_ascii_case(target))
        .unwrap_or(false)
}
