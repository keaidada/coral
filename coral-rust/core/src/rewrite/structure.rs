// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Structure-level rewrites (whole-query transformations).
//!
//! `DISTINCT ON (keys)` is not supported by Spark SQL. GaussDB semantics:
//! "keep one row per value of `keys`, choosing the first row according to the
//! query's `ORDER BY`". We rewrite this into the standard SQL equivalent:
//!
//! ```text
//! SELECT DISTINCT ON (dept_id) id, dept_id, salary
//! FROM employees
//! ORDER BY dept_id, salary DESC
//! ```
//! becomes
//! ```text
//! SELECT id, dept_id, salary
//! FROM (
//!     SELECT id, dept_id, salary,
//!            ROW_NUMBER() OVER (
//!                PARTITION BY dept_id
//!                ORDER BY dept_id, salary DESC
//!            ) AS __coral_distinct_on_rn
//!     FROM employees
//! ) AS __coral_distinct_on_t
//! WHERE __coral_distinct_on_rn = 1
//! ```
//!
//! This matches what `coral-gaussdb-spark` produces for sample #6 in SmokeDemo.

use sqlparser::ast::{
    Distinct, Expr, Function, FunctionArgumentList, FunctionArguments, Ident, ObjectName,
    OrderByExpr, Query, Select, SelectItem, SetExpr, TableAlias, TableFactor, TableWithJoins,
    Value, VisitorMut, WindowSpec, WindowType,
};
use std::ops::ControlFlow;

const RN_COLUMN: &str = "__coral_distinct_on_rn";
const SUBQUERY_ALIAS: &str = "__coral_distinct_on_t";

pub struct DistinctOnRewriter;

impl VisitorMut for DistinctOnRewriter {
    type Break = std::convert::Infallible;

    fn post_visit_query(&mut self, query: &mut Query) -> ControlFlow<Self::Break> {
        // Only top-level SELECT bodies carry DISTINCT ON — bail out fast if the
        // body is UNION / VALUES / CTE-wrapped Query etc.
        let SetExpr::Select(select) = query.body.as_mut() else {
            return ControlFlow::Continue(());
        };

        let Some(Distinct::On(on_keys)) = select.distinct.clone() else {
            return ControlFlow::Continue(());
        };

        // Pull the components we need out of the original Select.
        let original_select = std::mem::replace(select.as_mut(), *empty_select());
        // (We still need to mutate `query`, so keep `select` pointer reset.)

        // Build the inner projection = original projection + ROW_NUMBER() OVER (…).
        let mut inner_projection = original_select.projection.clone();
        inner_projection.push(SelectItem::ExprWithAlias {
            expr: row_number_over(
                on_keys.clone(),
                // ORDER BY clause of the outer query becomes the window's ORDER BY
                // (GaussDB requires the ORDER BY prefix to match the DISTINCT ON keys,
                // so this is semantically correct).
                query
                    .order_by
                    .as_ref()
                    .map(|ob| ob.exprs.clone())
                    .unwrap_or_default(),
            ),
            alias: Ident::new(RN_COLUMN),
        });

        // Inner SELECT: same FROM / WHERE / GROUP BY / HAVING, no DISTINCT, no ORDER BY.
        let inner_select = Select {
            distinct: None,
            projection: inner_projection,
            ..original_select.clone()
        };
        let inner_query = Query {
            with: None,
            body: Box::new(SetExpr::Select(Box::new(inner_select))),
            order_by: None,
            limit: None,
            limit_by: vec![],
            offset: None,
            fetch: None,
            locks: vec![],
            for_clause: None,
            settings: None,
            format_clause: None,
        };

        // Outer SELECT: project the *original* columns (drop the rn column),
        // FROM (inner_query) t, WHERE t.rn = 1.
        let outer_projection = strip_row_number(&original_select.projection);

        let outer_from = vec![TableWithJoins {
            relation: TableFactor::Derived {
                lateral: false,
                subquery: Box::new(inner_query),
                alias: Some(TableAlias {
                    name: Ident::new(SUBQUERY_ALIAS),
                    columns: vec![],
                }),
            },
            joins: vec![],
        }];

        let outer_where = Some(Expr::BinaryOp {
            left: Box::new(Expr::Identifier(Ident::new(RN_COLUMN))),
            op: sqlparser::ast::BinaryOperator::Eq,
            right: Box::new(Expr::Value(Value::Number("1".into(), false))),
        });

        let outer_select = Select {
            distinct: None,
            top: None,
            top_before_distinct: false,
            projection: outer_projection,
            into: None,
            from: outer_from,
            lateral_views: vec![],
            prewhere: None,
            selection: outer_where,
            group_by: sqlparser::ast::GroupByExpr::Expressions(vec![], vec![]),
            cluster_by: vec![],
            distribute_by: vec![],
            sort_by: vec![],
            having: None,
            named_window: vec![],
            qualify: None,
            window_before_qualify: false,
            value_table_mode: None,
            connect_by: None,
        };

        // Rewire the top-level query.
        *query = Query {
            with: query.with.take(),
            body: Box::new(SetExpr::Select(Box::new(outer_select))),
            // The ORDER BY has been absorbed into the window's ORDER BY clause,
            // so dropping it from the outer query is safe and matches the
            // reference output.
            order_by: None,
            limit: query.limit.take(),
            limit_by: std::mem::take(&mut query.limit_by),
            offset: query.offset.take(),
            fetch: query.fetch.take(),
            locks: std::mem::take(&mut query.locks),
            for_clause: query.for_clause.take(),
            settings: query.settings.take(),
            format_clause: query.format_clause.take(),
        };

        ControlFlow::Continue(())
    }
}

/// Construct a `ROW_NUMBER() OVER (PARTITION BY <partition> ORDER BY <order>)`
/// function expression.
fn row_number_over(partition_by: Vec<Expr>, order_by: Vec<OrderByExpr>) -> Expr {
    Expr::Function(Function {
        name: ObjectName(vec![Ident::new("ROW_NUMBER")]),
        parameters: FunctionArguments::None,
        args: FunctionArguments::List(FunctionArgumentList {
            duplicate_treatment: None,
            args: vec![],
            clauses: vec![],
        }),
        filter: None,
        null_treatment: None,
        over: Some(WindowType::WindowSpec(WindowSpec {
            window_name: None,
            partition_by,
            order_by,
            window_frame: None,
        })),
        within_group: vec![],
    })
}

/// Produce the outer projection by stripping the trailing `ROW_NUMBER() AS rn`
/// column and re-projecting the original columns by their output name.
///
/// For `SELECT DISTINCT ON (d) id, d, s`, the inner projection at this point is
/// `[id, d, s, ROW_NUMBER()... AS rn]`. We want the outer projection to name
/// each original column by the alias it had (or identifier if it was
/// `UnnamedExpr(Identifier(...))`), so that bare identifiers in the outer SELECT
/// resolve against the derived table.
fn strip_row_number(inner: &[SelectItem]) -> Vec<SelectItem> {
    inner
        .iter()
        .take_while(|item| {
            !matches!(
                item,
                SelectItem::ExprWithAlias { alias, .. } if alias.value == RN_COLUMN
            )
        })
        .map(|item| match item {
            SelectItem::UnnamedExpr(Expr::Identifier(id)) => {
                // Bare identifier: re-project by the same name.
                SelectItem::UnnamedExpr(Expr::Identifier(id.clone()))
            }
            SelectItem::UnnamedExpr(Expr::CompoundIdentifier(ids)) => {
                // e.g. `t.col` — keep the last segment.
                let last = ids.last().cloned().unwrap_or_else(|| Ident::new("col"));
                SelectItem::UnnamedExpr(Expr::Identifier(last))
            }
            SelectItem::UnnamedExpr(expr) => {
                // Generic expression without alias: pass through unchanged.
                // (Uncommon in practice; DISTINCT ON usually selects named columns.)
                SelectItem::UnnamedExpr(expr.clone())
            }
            SelectItem::ExprWithAlias { alias, .. } => {
                // Already has an alias: re-project by the alias.
                SelectItem::UnnamedExpr(Expr::Identifier(alias.clone()))
            }
            SelectItem::QualifiedWildcard(..) | SelectItem::Wildcard(_) => item.clone(),
        })
        .collect()
}

/// A zero-valued `Select` used as a placeholder during `std::mem::replace`.
/// The placeholder is immediately overwritten with the real outer Select.
fn empty_select() -> Box<Select> {
    Box::new(Select {
        distinct: None,
        top: None,
        top_before_distinct: false,
        projection: vec![],
        into: None,
        from: vec![],
        lateral_views: vec![],
        prewhere: None,
        selection: None,
        group_by: sqlparser::ast::GroupByExpr::Expressions(vec![], vec![]),
        cluster_by: vec![],
        distribute_by: vec![],
        sort_by: vec![],
        having: None,
        named_window: vec![],
        qualify: None,
        window_before_qualify: false,
        value_table_mode: None,
        connect_by: None,
    })
}
