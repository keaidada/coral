// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! AST → Node tree walker.
//!
//! A dialect-neutral tree of `(label, kind, children)` triples that
//! the DOT / PlantUML renderers consume. Kept simple so both renderers
//! can stay ~60 lines each.

use sqlparser::ast::{
    Expr, GroupByExpr, OrderByExpr, Query, Select, SelectItem, SetExpr, Statement, TableFactor,
    TableWithJoins,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeKind {
    Statement,
    Query,
    Select,
    From,
    Table,
    Join,
    Filter,
    GroupBy,
    Having,
    OrderBy,
    Limit,
    Projection,
    SetOp,
    Cte,
    Expression,
    Subquery,
    Values,
    Misc,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub label: String,
    pub kind: NodeKind,
    pub children: Vec<Node>,
}

impl Node {
    fn leaf<S: Into<String>>(label: S, kind: NodeKind) -> Self {
        Self {
            label: label.into(),
            kind,
            children: vec![],
        }
    }

    fn with_children<S: Into<String>>(label: S, kind: NodeKind, children: Vec<Node>) -> Self {
        Self {
            label: label.into(),
            kind,
            children,
        }
    }
}

// -------------------------------------------------------------------
// Entry point per statement
// -------------------------------------------------------------------

pub fn walk_statement(stmt: &Statement) -> Node {
    match stmt {
        Statement::Query(q) => walk_query(q),
        Statement::Insert(ins) => Node::with_children(
            format!("INSERT INTO {}", ins.table_name),
            NodeKind::Statement,
            ins.source.as_ref().map(|q| vec![walk_query(q)]).unwrap_or_default(),
        ),
        Statement::Update { table, .. } => {
            Node::leaf(format!("UPDATE {}", table.relation), NodeKind::Statement)
        }
        Statement::Delete(del) => Node::leaf(
            format!("DELETE ({} targets)", del.tables.len()),
            NodeKind::Statement,
        ),
        Statement::CreateTable(ct) => Node::leaf(
            format!("CREATE TABLE {}", ct.name),
            NodeKind::Statement,
        ),
        Statement::CreateView { name, query, .. } => Node::with_children(
            format!("CREATE VIEW {name}"),
            NodeKind::Statement,
            vec![walk_query(query)],
        ),
        Statement::Merge { table, .. } => {
            Node::leaf(format!("MERGE {table}"), NodeKind::Statement)
        }
        Statement::AlterTable { name, .. } => {
            Node::leaf(format!("ALTER TABLE {name}"), NodeKind::Statement)
        }
        Statement::Drop { object_type, names, .. } => Node::leaf(
            format!(
                "DROP {:?} {}",
                object_type,
                names
                    .iter()
                    .map(|n| n.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            NodeKind::Statement,
        ),
        other => Node::leaf(summarize(other), NodeKind::Misc),
    }
}

fn summarize<T: std::fmt::Display>(t: &T) -> String {
    let s = format!("{t}");
    s.lines().next().unwrap_or(&s).chars().take(60).collect()
}

// -------------------------------------------------------------------
// Query
// -------------------------------------------------------------------

fn walk_query(q: &Query) -> Node {
    let mut children = Vec::new();
    if let Some(with) = &q.with {
        let ctes: Vec<Node> = with
            .cte_tables
            .iter()
            .map(|cte| {
                Node::with_children(
                    format!("CTE {}", cte.alias.name.value),
                    NodeKind::Cte,
                    vec![walk_query(&cte.query)],
                )
            })
            .collect();
        children.push(Node::with_children(
            format!("WITH ({} CTE{})", ctes.len(), if ctes.len() == 1 { "" } else { "s" }),
            NodeKind::Cte,
            ctes,
        ));
    }
    children.push(walk_set_expr(&q.body));
    if let Some(ord) = &q.order_by {
        children.push(walk_order_by(&ord.exprs));
    }
    if let Some(lim) = &q.limit {
        children.push(Node::leaf(format!("LIMIT {lim}"), NodeKind::Limit));
    }
    Node::with_children("Query", NodeKind::Query, children)
}

fn walk_set_expr(s: &SetExpr) -> Node {
    match s {
        SetExpr::Select(select) => walk_select(select),
        SetExpr::SetOperation {
            op,
            set_quantifier,
            left,
            right,
        } => Node::with_children(
            format!("{op:?} {set_quantifier:?}"),
            NodeKind::SetOp,
            vec![walk_set_expr(left), walk_set_expr(right)],
        ),
        SetExpr::Query(inner) => Node::with_children(
            "Subquery",
            NodeKind::Subquery,
            vec![walk_query(inner)],
        ),
        SetExpr::Values(values) => Node::leaf(
            format!("VALUES ({} rows)", values.rows.len()),
            NodeKind::Values,
        ),
        SetExpr::Insert(_) | SetExpr::Update(_) | SetExpr::Table(_) => {
            Node::leaf(summarize(s), NodeKind::Misc)
        }
    }
}

fn walk_select(s: &Select) -> Node {
    let mut children = Vec::new();

    // FROM
    if !s.from.is_empty() {
        let from_children: Vec<Node> = s.from.iter().map(walk_table_with_joins).collect();
        children.push(Node::with_children(
            format!("FROM ({} source{})", s.from.len(), if s.from.len() == 1 { "" } else { "s" }),
            NodeKind::From,
            from_children,
        ));
    }

    // Projection
    let proj_children: Vec<Node> = s.projection.iter().map(walk_select_item).collect();
    let proj_count = proj_children.len();
    children.push(Node::with_children(
        format!("SELECT ({proj_count} col{})", if proj_count == 1 { "" } else { "s" }),
        NodeKind::Projection,
        proj_children,
    ));

    if let Some(filter) = &s.selection {
        children.push(Node::with_children(
            "WHERE",
            NodeKind::Filter,
            vec![walk_expr(filter)],
        ));
    }

    match &s.group_by {
        GroupByExpr::Expressions(exprs, _) if !exprs.is_empty() => {
            let gb_children: Vec<Node> = exprs.iter().map(walk_expr).collect();
            children.push(Node::with_children("GROUP BY", NodeKind::GroupBy, gb_children));
        }
        GroupByExpr::All(_) => {
            children.push(Node::leaf("GROUP BY ALL", NodeKind::GroupBy));
        }
        _ => {}
    }

    if let Some(having) = &s.having {
        children.push(Node::with_children(
            "HAVING",
            NodeKind::Having,
            vec![walk_expr(having)],
        ));
    }

    Node::with_children("Select", NodeKind::Select, children)
}

fn walk_select_item(item: &SelectItem) -> Node {
    match item {
        SelectItem::UnnamedExpr(e) => walk_expr(e),
        SelectItem::ExprWithAlias { expr, alias } => Node::with_children(
            format!("AS {}", alias.value),
            NodeKind::Projection,
            vec![walk_expr(expr)],
        ),
        SelectItem::Wildcard(_) => Node::leaf("*", NodeKind::Projection),
        SelectItem::QualifiedWildcard(name, _) => {
            Node::leaf(format!("{name}.*"), NodeKind::Projection)
        }
    }
}

fn walk_table_with_joins(twj: &TableWithJoins) -> Node {
    let mut kids = vec![walk_table_factor(&twj.relation)];
    for j in &twj.joins {
        kids.push(Node::with_children(
            format!("{:?}", j.join_operator).chars().take(30).collect::<String>(),
            NodeKind::Join,
            vec![walk_table_factor(&j.relation)],
        ));
    }
    if kids.len() == 1 {
        kids.into_iter().next().unwrap()
    } else {
        Node::with_children("Joins", NodeKind::From, kids)
    }
}

fn walk_table_factor(f: &TableFactor) -> Node {
    match f {
        TableFactor::Table { name, alias, .. } => {
            let label = alias
                .as_ref()
                .map(|a| format!("{name} AS {}", a.name.value))
                .unwrap_or_else(|| name.to_string());
            Node::leaf(label, NodeKind::Table)
        }
        TableFactor::Derived { subquery, alias, .. } => {
            let lbl = alias
                .as_ref()
                .map(|a| format!("Derived AS {}", a.name.value))
                .unwrap_or_else(|| "Derived".to_string());
            Node::with_children(lbl, NodeKind::Subquery, vec![walk_query(subquery)])
        }
        other => Node::leaf(summarize(other), NodeKind::Table),
    }
}

fn walk_order_by(exprs: &[OrderByExpr]) -> Node {
    let kids: Vec<Node> = exprs
        .iter()
        .map(|o| {
            let dir = match o.asc {
                Some(true) => " ASC",
                Some(false) => " DESC",
                None => "",
            };
            Node::with_children(
                format!("sort{dir}"),
                NodeKind::OrderBy,
                vec![walk_expr(&o.expr)],
            )
        })
        .collect();
    Node::with_children(
        format!("ORDER BY ({} key{})", kids.len(), if kids.len() == 1 { "" } else { "s" }),
        NodeKind::OrderBy,
        kids,
    )
}

// -------------------------------------------------------------------
// Expressions
// -------------------------------------------------------------------

fn walk_expr(e: &Expr) -> Node {
    match e {
        Expr::Identifier(i) => Node::leaf(i.value.clone(), NodeKind::Expression),
        Expr::CompoundIdentifier(parts) => Node::leaf(
            parts.iter().map(|i| i.value.as_str()).collect::<Vec<_>>().join("."),
            NodeKind::Expression,
        ),
        Expr::Value(v) => Node::leaf(format!("{v}"), NodeKind::Expression),
        Expr::BinaryOp { left, op, right } => Node::with_children(
            format!("{op}"),
            NodeKind::Expression,
            vec![walk_expr(left), walk_expr(right)],
        ),
        Expr::UnaryOp { op, expr } => Node::with_children(
            format!("{op}"),
            NodeKind::Expression,
            vec![walk_expr(expr)],
        ),
        Expr::Function(f) => {
            let kids: Vec<Node> = match &f.args {
                sqlparser::ast::FunctionArguments::List(list) => list
                    .args
                    .iter()
                    .map(|a| match a {
                        sqlparser::ast::FunctionArg::Unnamed(
                            sqlparser::ast::FunctionArgExpr::Expr(e),
                        ) => walk_expr(e),
                        other => Node::leaf(summarize(other), NodeKind::Expression),
                    })
                    .collect(),
                _ => vec![],
            };
            Node::with_children(format!("{}()", f.name), NodeKind::Expression, kids)
        }
        Expr::Cast { expr, data_type, .. } => Node::with_children(
            format!("CAST AS {data_type}"),
            NodeKind::Expression,
            vec![walk_expr(expr)],
        ),
        Expr::Case { conditions, results, else_result, .. } => {
            let mut kids = Vec::new();
            for (c, r) in conditions.iter().zip(results.iter()) {
                kids.push(Node::with_children(
                    "WHEN",
                    NodeKind::Expression,
                    vec![walk_expr(c), walk_expr(r)],
                ));
            }
            if let Some(er) = else_result {
                kids.push(Node::with_children(
                    "ELSE",
                    NodeKind::Expression,
                    vec![walk_expr(er)],
                ));
            }
            Node::with_children("CASE", NodeKind::Expression, kids)
        }
        Expr::Subquery(q) => Node::with_children(
            "Subquery",
            NodeKind::Subquery,
            vec![walk_query(q)],
        ),
        Expr::IsNull(inner) => Node::with_children(
            "IS NULL",
            NodeKind::Expression,
            vec![walk_expr(inner)],
        ),
        Expr::IsNotNull(inner) => Node::with_children(
            "IS NOT NULL",
            NodeKind::Expression,
            vec![walk_expr(inner)],
        ),
        other => Node::leaf(summarize(other), NodeKind::Expression),
    }
}
