// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Optional catalog layer.
//!
//! The pure [`translate`](crate::translate) pipeline is text->text: it does
//! not know which columns actually exist, it just rewrites syntax. That's
//! enough for 80% of use cases, but sometimes you want to catch typos before
//! a query hits the cluster.
//!
//! This module provides a tiny schema-aware validation pass that works off
//! the same in-memory format the Java tree uses for its
//! `CoralGaussDBToSpark.createLocal(sql, catalog)` API:
//!
//! ```ignore
//! catalog := { db_name: { table_name: [ "col_name|hive_type", ... ] } }
//! ```
//!
//! For example, to describe a `default.employees` table with three columns:
//!
//! ```rust
//! use coral_core::catalog::InMemoryCatalog;
//!
//! let catalog = InMemoryCatalog::from_pairs(&[
//!     ("default", "employees", &[
//!         "id|int",
//!         "name|string",
//!         "dept_id|int",
//!     ]),
//!     ("default", "departments", &[
//!         "id|int",
//!         "name|string",
//!     ]),
//! ]);
//!
//! let issues = coral_core::translate_with_catalog(
//!     "SELECT id, dpt_id FROM default.employees",
//!     &catalog,
//! ).unwrap();
//! // issues[0] warns about `dpt_id` (typo for `dept_id`).
//! ```
//!
//! The catalog is **advisory**: unknown columns are reported as
//! [`ValidationIssue`] warnings, they do NOT fail the translation. The
//! translated Spark SQL is returned alongside the issues so callers can log
//! them and move on, or escalate.

use std::collections::HashMap;

use sqlparser::ast::{
    Expr, Ident, ObjectName, Query, SetExpr, TableFactor, TableWithJoins, VisitMut, VisitorMut,
};
use std::ops::ControlFlow;

/// A trait so callers can plug in their own catalog source (Hive Metastore
/// remote, Unity Catalog, etc.) instead of the in-memory default.
///
/// All lookups are case-insensitive — GaussDB/PG fold unquoted identifiers to
/// lowercase, and Spark is case-preserving but case-insensitive on resolve.
pub trait Catalog {
    /// Return the column names for `db.table`, or None if the table is
    /// unknown. Column names are returned in the catalog's canonical case.
    fn columns_of(&self, db: &str, table: &str) -> Option<&[String]>;
}

/// In-memory catalog backed by the `{db: {table: [col|type, ...]}}` shape
/// that the Java LocalMetastore consumes.
#[derive(Debug, Default, Clone)]
pub struct InMemoryCatalog {
    /// db (lowercased) -> table (lowercased) -> (column-name, hive-type) pairs.
    tables: HashMap<String, HashMap<String, TableSchema>>,
}

#[derive(Debug, Clone)]
struct TableSchema {
    columns: Vec<String>,
    column_types: Vec<String>,
}

impl InMemoryCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Convenience constructor that takes `(db, table, &["col|type", ...])`
    /// triples — same format as the Java `LocalMetastore` test fixtures so
    /// you can copy/paste test data between trees.
    pub fn from_pairs(entries: &[(&str, &str, &[&str])]) -> Self {
        let mut me = Self::new();
        for (db, table, cols) in entries {
            me.add_table(
                db,
                table,
                cols.iter().map(|s| s.to_string()).collect(),
            );
        }
        me
    }

    /// Register a table. `col_type_pairs` entries are of the form
    /// `"column_name|hive_type"` (matching the Java API).
    pub fn add_table(&mut self, db: &str, table: &str, col_type_pairs: Vec<String>) {
        let db_key = db.to_ascii_lowercase();
        let table_key = table.to_ascii_lowercase();
        let (columns, column_types): (Vec<_>, Vec<_>) = col_type_pairs
            .iter()
            .map(|s| {
                let mut iter = s.splitn(2, '|');
                let col = iter.next().unwrap_or("").to_string();
                let ty = iter.next().unwrap_or("").to_string();
                (col, ty)
            })
            .unzip();
        self.tables
            .entry(db_key)
            .or_default()
            .insert(table_key, TableSchema { columns, column_types });
    }

    /// Look up a table and return its original (case-preserved) column names.
    pub fn columns(&self, db: &str, table: &str) -> Option<&[String]> {
        self.tables
            .get(&db.to_ascii_lowercase())
            .and_then(|t| t.get(&table.to_ascii_lowercase()))
            .map(|s| s.columns.as_slice())
    }

    /// Column types paired with column names (both Vecs, same index).
    pub fn column_types(&self, db: &str, table: &str) -> Option<(&[String], &[String])> {
        self.tables
            .get(&db.to_ascii_lowercase())
            .and_then(|t| t.get(&table.to_ascii_lowercase()))
            .map(|s| (s.columns.as_slice(), s.column_types.as_slice()))
    }
}

impl Catalog for InMemoryCatalog {
    fn columns_of(&self, db: &str, table: &str) -> Option<&[String]> {
        self.columns(db, table)
    }
}

/// Soft validation warnings produced by [`validate_against`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationIssue {
    /// The query references `db.table` but the catalog has no such table.
    UnknownTable { db: String, table: String },
    /// The query references column `column` on table `db.table` but that
    /// column is not in the catalog.
    UnknownColumn {
        db: String,
        table: String,
        column: String,
        /// Suggested closest match (Levenshtein distance 1-2) if any.
        did_you_mean: Option<String>,
    },
}

/// Visit every referenced table/column in `stmt` and return any issues that
/// the `catalog` flags. Does NOT mutate the statement.
pub fn validate_against<C: Catalog>(
    stmt: &sqlparser::ast::Statement,
    catalog: &C,
) -> Vec<ValidationIssue> {
    let mut v = Validator {
        catalog,
        issues: vec![],
        scopes: vec![],
        cte_names: vec![],
        cte_counts: vec![],
    };
    let mut cloned = stmt.clone();
    let _ = cloned.visit(&mut v);
    v.issues
}

/// Walker that tracks the set of tables visible in the current query's FROM
/// clause so it can resolve `a.col` references.
struct Validator<'a, C: Catalog> {
    catalog: &'a C,
    issues: Vec<ValidationIssue>,
    /// Each scope is a Vec of TableRefs visible in the current query's FROM
    /// clause. Scopes stack for nested subqueries.
    scopes: Vec<Vec<TableRef>>,
    /// CTE names visible at this point (flattened across all ancestor queries).
    /// Checked before reporting an "unknown table" — CTE-backed relations
    /// don't live in the catalog.
    cte_names: Vec<String>,
    /// Count of CTE names added by each query's `WITH` clause, so
    /// `post_visit_query` knows how many entries to drop.
    cte_counts: Vec<usize>,
}

#[derive(Debug, Clone)]
struct TableRef {
    db: String,
    table: String,
    /// The unqualified handle the user can use in WHERE/SELECT: alias if
    /// there is one, otherwise the table's bare name.
    handle: String,
}

impl<C: Catalog> VisitorMut for Validator<'_, C> {
    type Break = std::convert::Infallible;

    fn pre_visit_query(&mut self, query: &mut Query) -> ControlFlow<Self::Break> {
        // Register CTE names first so the scope doesn't try to catalog-look
        // them up.
        let cte_added: Vec<String> = query
            .with
            .as_ref()
            .map(|w| w.cte_tables.iter().map(|c| c.alias.name.value.clone()).collect())
            .unwrap_or_default();
        let added_count = cte_added.len();
        self.cte_names.extend(cte_added);
        self.cte_counts.push(added_count);

        // Build a new scope from the query's FROM clause, skipping CTE refs.
        let scope = collect_tables(query);
        for t in &scope {
            if self.is_cte(&t.table) {
                continue;
            }
            if self.catalog.columns_of(&t.db, &t.table).is_none() {
                self.issues.push(ValidationIssue::UnknownTable {
                    db: t.db.clone(),
                    table: t.table.clone(),
                });
            }
        }
        self.scopes.push(scope);
        ControlFlow::Continue(())
    }

    fn post_visit_query(&mut self, _query: &mut Query) -> ControlFlow<Self::Break> {
        self.scopes.pop();
        if let Some(n) = self.cte_counts.pop() {
            let new_len = self.cte_names.len().saturating_sub(n);
            self.cte_names.truncate(new_len);
        }
        ControlFlow::Continue(())
    }

    fn pre_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        match expr {
            Expr::CompoundIdentifier(ids) if ids.len() == 2 => {
                self.check_qualified(&ids[0], &ids[1]);
            }
            _ => {}
        }
        ControlFlow::Continue(())
    }
}

impl<C: Catalog> Validator<'_, C> {
    fn is_cte(&self, name: &str) -> bool {
        self.cte_names
            .iter()
            .any(|c| c.eq_ignore_ascii_case(name))
    }

    fn check_qualified(&mut self, qualifier: &Ident, col: &Ident) {
        // Find the referenced table via the current (innermost) scope stack.
        for scope in self.scopes.iter().rev() {
            if let Some(t) = scope
                .iter()
                .find(|t| t.handle.eq_ignore_ascii_case(&qualifier.value))
            {
                if self.is_cte(&t.table) {
                    // CTE columns aren't in the catalog — accept silently.
                    return;
                }
                if let Some(cols) = self.catalog.columns_of(&t.db, &t.table) {
                    if !cols.iter().any(|c| c.eq_ignore_ascii_case(&col.value)) {
                        let suggestion = best_match(&col.value, cols);
                        self.issues.push(ValidationIssue::UnknownColumn {
                            db: t.db.clone(),
                            table: t.table.clone(),
                            column: col.value.clone(),
                            did_you_mean: suggestion,
                        });
                    }
                }
                return;
            }
        }
        // Qualifier didn't match any table in scope. This could be a
        // CTE/derived table; silently accept rather than cry wolf.
    }
}

/// Extract the list of tables visible in `query`'s FROM clause (one level
/// only — subqueries build their own scope in the visitor).
fn collect_tables(query: &Query) -> Vec<TableRef> {
    let SetExpr::Select(select) = query.body.as_ref() else {
        return vec![];
    };
    let mut out = vec![];
    for twj in &select.from {
        collect_from_twj(twj, &mut out);
    }
    out
}

fn collect_from_twj(twj: &TableWithJoins, out: &mut Vec<TableRef>) {
    collect_from_factor(&twj.relation, out);
    for j in &twj.joins {
        collect_from_factor(&j.relation, out);
    }
}

fn collect_from_factor(factor: &TableFactor, out: &mut Vec<TableRef>) {
    if let TableFactor::Table { name, alias, .. } = factor {
        let (db, table) = split_db_table(name);
        let handle = alias
            .as_ref()
            .map(|a| a.name.value.clone())
            .unwrap_or_else(|| table.clone());
        out.push(TableRef { db, table, handle });
    }
    // Derived/function/unnest subqueries are out of scope for this simple
    // validator; their own visit_query call will build its own scope.
}

/// "default.employees" -> ("default", "employees").
/// "employees"         -> ("default", "employees") — assumes the default db.
fn split_db_table(name: &ObjectName) -> (String, String) {
    match name.0.len() {
        1 => ("default".into(), name.0[0].value.clone()),
        _ => (
            name.0[name.0.len() - 2].value.clone(),
            name.0[name.0.len() - 1].value.clone(),
        ),
    }
}

/// Levenshtein distance — helper for "did you mean?" suggestions.
fn lev(a: &str, b: &str) -> usize {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (n, m) = (a.len(), b.len());
    if n == 0 {
        return m;
    }
    if m == 0 {
        return n;
    }
    let mut prev: Vec<usize> = (0..=m).collect();
    let mut curr = vec![0; m + 1];
    for i in 1..=n {
        curr[0] = i;
        for j in 1..=m {
            let cost = if a[i - 1].eq_ignore_ascii_case(&b[j - 1]) {
                0
            } else {
                1
            };
            curr[j] = (curr[j - 1] + 1)
                .min(prev[j] + 1)
                .min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[m]
}

/// Return the closest column name (Levenshtein ≤ 2) if any.
fn best_match(target: &str, candidates: &[String]) -> Option<String> {
    candidates
        .iter()
        .map(|c| (c.clone(), lev(target, c)))
        .filter(|(_, d)| *d <= 2)
        .min_by_key(|(_, d)| *d)
        .map(|(c, _)| c)
}

// Unused; kept to silence a "struct field never read" warning that appears
// on some rustc versions.
#[allow(dead_code)]
fn _touch(_e: &Expr) {}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    fn parse(sql: &str) -> sqlparser::ast::Statement {
        Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap().remove(0)
    }

    fn sample_catalog() -> InMemoryCatalog {
        InMemoryCatalog::from_pairs(&[
            ("default", "employees", &["id|int", "name|string", "dept_id|int"]),
            ("default", "departments", &["id|int", "name|string"]),
        ])
    }

    #[test]
    fn unknown_table_detected() {
        let cat = sample_catalog();
        let stmt = parse("SELECT * FROM default.nonesuch");
        let issues = validate_against(&stmt, &cat);
        assert_eq!(
            issues,
            vec![ValidationIssue::UnknownTable {
                db: "default".into(),
                table: "nonesuch".into(),
            }]
        );
    }

    #[test]
    fn unknown_column_with_suggestion() {
        let cat = sample_catalog();
        let stmt = parse("SELECT e.dpt_id FROM default.employees e");
        let issues = validate_against(&stmt, &cat);
        assert_eq!(
            issues,
            vec![ValidationIssue::UnknownColumn {
                db: "default".into(),
                table: "employees".into(),
                column: "dpt_id".into(),
                did_you_mean: Some("dept_id".into()),
            }]
        );
    }

    #[test]
    fn known_qualified_column_ok() {
        let cat = sample_catalog();
        let stmt = parse("SELECT e.name FROM default.employees e");
        assert!(validate_against(&stmt, &cat).is_empty());
    }

    #[test]
    fn bare_table_name_uses_default_db() {
        let cat = sample_catalog();
        let stmt = parse("SELECT e.name FROM employees e");
        assert!(validate_against(&stmt, &cat).is_empty());
    }

    #[test]
    fn case_insensitive_match() {
        let cat = sample_catalog();
        let stmt = parse("SELECT E.NAME FROM DEFAULT.EMPLOYEES e");
        assert!(validate_against(&stmt, &cat).is_empty());
    }

    #[test]
    fn unqualified_column_ignored() {
        // We don't try to resolve unqualified references — too many
        // false positives with CTEs/derived tables.
        let cat = sample_catalog();
        let stmt = parse("SELECT unknown_col FROM default.employees");
        assert!(validate_against(&stmt, &cat).is_empty());
    }

    #[test]
    fn levenshtein_basic() {
        assert_eq!(lev("dept_id", "dept_id"), 0);
        assert_eq!(lev("dpt_id", "dept_id"), 1);
        assert_eq!(lev("dept_id", "deptid"), 1);
        assert_eq!(lev("abc", "xyz"), 3);
    }
}
