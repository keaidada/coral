// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! The rewriter pipeline.
//!
//! Algorithm:
//!   1. Parse `sql` into statements. We operate on the first statement
//!      only — the Java tree does the same.
//!   2. Walk the AST (via a `VisitorMut`) to discover the set of base
//!      tables referenced. Each `TableFactor::Table` node records a
//!      path to itself (as a numeric ID we assign in walk order).
//!   3. Guard on `MAX_TABLES`.
//!   4. For each non-empty subset `S` of the tables, clone the AST,
//!      mutate the referenced names in positions in `S` by appending
//!      the delta suffix, render, and collect.
//!   5. Join the rendered variants with `UNION ALL`.

use sqlparser::ast::{ObjectName, Statement, TableFactor, VisitMut, VisitorMut};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::ops::ControlFlow;

use crate::error::{IncrementalError, Result};

/// Rewrite `sql` into its incremental UNION-ALL form using the default
/// delta suffix `"_delta"`.
pub fn incremental_sql(sql: &str) -> Result<String> {
    incremental_sql_with(sql, "_delta")
}

/// Variant that lets the caller pick the delta suffix (e.g. `"__delta"`,
/// `"_incr"`). Leading-underscore is fine; the function just appends
/// the string to each table's **last** identifier segment.
pub fn incremental_sql_with(sql: &str, delta_suffix: &str) -> Result<String> {
    let dialect = PostgreSqlDialect {};
    let mut stmts = Parser::parse_sql(&dialect, sql)?;
    if stmts.is_empty() {
        return Err(IncrementalError::NoTables);
    }
    let first = &mut stmts[0];

    // Pass 1: count tables.
    let mut counter = TableCounter { count: 0 };
    let _ = first.visit(&mut counter);
    let n = counter.count;
    if n == 0 {
        return Err(IncrementalError::NoTables);
    }
    if n > crate::MAX_TABLES {
        return Err(IncrementalError::TooManyTables(n));
    }

    // Pass 2: for each subset, render a fresh rewrite.
    let mut branches = Vec::with_capacity((1usize << n) - 1);
    for mask in 1u32..(1u32 << n) {
        let mut clone = first.clone();
        let mut r = TableRewriter {
            visited: 0,
            mask,
            suffix: delta_suffix,
        };
        let _ = clone.visit(&mut r);
        branches.push(clone.to_string());
    }

    Ok(branches.join("\nUNION ALL\n"))
}

// -------------------------------------------------------------------
// AST visitors
// -------------------------------------------------------------------

struct TableCounter {
    count: usize,
}

impl VisitorMut for TableCounter {
    type Break = std::convert::Infallible;

    fn post_visit_table_factor(&mut self, factor: &mut TableFactor) -> ControlFlow<Self::Break> {
        if matches!(factor, TableFactor::Table { .. }) {
            self.count += 1;
        }
        ControlFlow::Continue(())
    }
}

struct TableRewriter<'a> {
    /// How many `TableFactor::Table` nodes we've visited so far.
    visited: u32,
    /// Bitmask — bit `i` set means "apply delta suffix to the i-th
    /// table we encounter".
    mask: u32,
    suffix: &'a str,
}

impl<'a> VisitorMut for TableRewriter<'a> {
    type Break = std::convert::Infallible;

    fn post_visit_table_factor(&mut self, factor: &mut TableFactor) -> ControlFlow<Self::Break> {
        if let TableFactor::Table { name, .. } = factor {
            let idx = self.visited;
            self.visited += 1;
            if (self.mask >> idx) & 1 == 1 {
                rename_last_segment(name, self.suffix);
            }
        }
        ControlFlow::Continue(())
    }
}

/// Append `suffix` to the last identifier segment of `name` (e.g.
/// `hr.employees` → `hr.employees_delta`).
fn rename_last_segment(name: &mut ObjectName, suffix: &str) {
    if let Some(last) = name.0.last_mut() {
        last.value = format!("{}{suffix}", last.value);
    }
}

// -------------------------------------------------------------------
// Silence unused imports lint
// -------------------------------------------------------------------

#[allow(dead_code)]
fn _keep_types_in_scope(_s: &Statement) {}
