// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Function rewrites specific to the **Trino** output target.
//!
//! Port of the rewrite rules from Java `coral-trino`'s
//! `CoralToTrinoSqlCallConverter` + `HiveToTrinoConverter`. These rules run
//! AFTER the Spark-compatible rewrites in `rewrite::functions`, so by the
//! time this visitor sees the AST, input like `NVL(a, b)` has already been
//! normalized to `COALESCE(a, b)` — which is what Trino wants too, so
//! there's nothing more for us to do.
//!
//! What this pass handles is the diff between **Spark SQL** and **Trino
//! SQL** that isn't already covered by the Spark-targeted rewrites:
//!
//! | Spark name               | Trino name / shape                        |
//! |--------------------------|-------------------------------------------|
//! | `RAND()` / `RAND(n)`     | `RANDOM()`                                |
//! | `RAND_INTEGER(n)`        | `RANDOM()`                                |
//! | `GET_JSON_OBJECT(j, p)`  | `JSON_EXTRACT(j, p)`                      |
//! | `ARRAY_CONTAINS(arr, v)` | `CONTAINS(arr, v)`                        |
//! | `ELEMENT_AT` / `item`    | `ELEMENT_AT` (same)                       |
//! | `BASE64(s)`              | `TO_BASE64(s)`                            |
//! | `UNBASE64(s)`            | `FROM_BASE64(s)`                          |
//! | `HEX(s)`                 | `TO_HEX(s)`                               |
//! | `UNHEX(s)`               | `FROM_HEX(s)`                             |
//! | `INSTR(s, p)`            | `STRPOS(s, p)`                            |
//! | `RLIKE x y` (op)         | `REGEXP_LIKE(x, y)` (function)            |
//! | `REGEXP_EXTRACT`         | `REGEXP_EXTRACT` (same, Trino regex only) |
//! | `PMOD(a, b)`             | `((a % b) + b) % b`                       |
//! | `DATE_ADD(date, n)`      | `DATE_ADD('day', n, CAST(date AS DATE))`  |
//! | `DATE_SUB(date, n)`      | `DATE_ADD('day', -n, CAST(date AS DATE))` |
//! | `DATEDIFF(a, b)`         | `DATE_DIFF('day', CAST(b AS DATE), CAST(a AS DATE))` |
//! | `COLLECT_LIST(x)`        | `ARRAY_AGG(x)`                            |
//! | `COLLECT_SET(x)`         | `ARRAY_AGG(DISTINCT x)`                   |
//! | `CONCAT_WS(s, arr)`      | `ARRAY_JOIN(arr, s)` when arr is ARRAY    |
//! | `TO_DATE(s)`             | `DATE(CAST(s AS TIMESTAMP))`              |
//!
//! Non-listed functions pass through untouched. Trino will reject any that
//! it genuinely doesn't know — we don't try to silently invent bindings.

use sqlparser::ast::{
    BinaryOperator, DataType, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArgumentList,
    FunctionArguments, Ident, ObjectName, UnaryOperator, Value, VisitorMut,
};
use std::ops::ControlFlow;

/// VisitorMut that applies the Spark → Trino diff in a single post-order pass.
pub struct TrinoFunctionRewriter;

impl VisitorMut for TrinoFunctionRewriter {
    type Break = std::convert::Infallible;

    fn post_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        // ---- 1) RLIKE operator -> REGEXP_LIKE function
        if let Expr::BinaryOp { left, op, right } = expr {
            if matches!(op, BinaryOperator::Custom(s) if s.eq_ignore_ascii_case("RLIKE")) {
                let l = std::mem::replace(left.as_mut(), dummy_expr());
                let r = std::mem::replace(right.as_mut(), dummy_expr());
                *expr = call("REGEXP_LIKE", vec![l, r]);
                return ControlFlow::Continue(());
            }
        }

        // ---- 2) Function-level rewrites
        if let Expr::Function(f) = expr {
            if let Some(replacement) = rewrite_function(f) {
                *expr = replacement;
            }
        }

        ControlFlow::Continue(())
    }
}

fn rewrite_function(f: &mut Function) -> Option<Expr> {
    let name = function_name_lower(f);
    match name.as_str() {
        "rand" | "rand_integer" => {
            // Trino RANDOM() takes no args; drop any.
            f.name = ObjectName(vec![Ident::new("RANDOM")]);
            f.args = FunctionArguments::List(empty_arg_list());
            None
        }
        "get_json_object" => {
            f.name = ObjectName(vec![Ident::new("JSON_EXTRACT")]);
            None
        }
        "array_contains" => {
            f.name = ObjectName(vec![Ident::new("CONTAINS")]);
            None
        }
        "base64" => {
            f.name = ObjectName(vec![Ident::new("TO_BASE64")]);
            None
        }
        "unbase64" => {
            f.name = ObjectName(vec![Ident::new("FROM_BASE64")]);
            None
        }
        "hex" => {
            f.name = ObjectName(vec![Ident::new("TO_HEX")]);
            None
        }
        "unhex" => {
            f.name = ObjectName(vec![Ident::new("FROM_HEX")]);
            None
        }
        "instr" => {
            f.name = ObjectName(vec![Ident::new("STRPOS")]);
            None
        }
        "collect_list" => {
            f.name = ObjectName(vec![Ident::new("ARRAY_AGG")]);
            None
        }
        "collect_set" => rewrite_collect_set(f),
        "pmod" => rewrite_pmod(f),
        "date_add" => rewrite_date_add_or_sub(f, /* negate */ false),
        "date_sub" => rewrite_date_add_or_sub(f, /* negate */ true),
        "datediff" => rewrite_datediff(f),
        "to_date" => rewrite_to_date(f),
        "concat_ws" => None, // left as-is; Trino supports CONCAT_WS
        _ => None,
    }
}

fn rewrite_collect_set(f: &mut Function) -> Option<Expr> {
    let args = take_all_positional_args(f)?;
    if args.len() != 1 {
        return Some(call_with("COLLECT_SET", args));
    }
    // ARRAY_AGG(DISTINCT x)
    let x = args.into_iter().next().unwrap();
    let list = FunctionArgumentList {
        duplicate_treatment: Some(sqlparser::ast::DuplicateTreatment::Distinct),
        args: vec![FunctionArg::Unnamed(FunctionArgExpr::Expr(x))],
        clauses: vec![],
    };
    Some(Expr::Function(Function {
        name: ObjectName(vec![Ident::new("ARRAY_AGG")]),
        parameters: FunctionArguments::None,
        args: FunctionArguments::List(list),
        filter: None,
        null_treatment: None,
        over: None,
        within_group: vec![],
    }))
}

fn rewrite_pmod(f: &mut Function) -> Option<Expr> {
    // PMOD(a, b) -> ((a % b) + b) % b
    let mut args = take_positional_args(f, 2)?;
    let b = args.remove(1);
    let a = args.remove(0);
    let inner = Expr::BinaryOp {
        left: Box::new(a),
        op: BinaryOperator::Modulo,
        right: Box::new(b.clone()),
    };
    let plus = Expr::BinaryOp {
        left: Box::new(Expr::Nested(Box::new(inner))),
        op: BinaryOperator::Plus,
        right: Box::new(b.clone()),
    };
    Some(Expr::BinaryOp {
        left: Box::new(Expr::Nested(Box::new(plus))),
        op: BinaryOperator::Modulo,
        right: Box::new(b),
    })
}

fn rewrite_date_add_or_sub(f: &mut Function, negate: bool) -> Option<Expr> {
    // Spark: DATE_ADD(date, n)    → Trino: DATE_ADD('day',  n, CAST(date AS DATE))
    // Spark: DATE_SUB(date, n)    → Trino: DATE_ADD('day', -n, CAST(date AS DATE))
    let mut args = take_positional_args(f, 2)?;
    let n = args.remove(1);
    let d = args.remove(0);
    let n_signed = if negate {
        Expr::UnaryOp {
            op: UnaryOperator::Minus,
            expr: Box::new(n),
        }
    } else {
        n
    };
    let day_lit = Expr::Value(Value::SingleQuotedString("day".to_string()));
    let date_cast = Expr::Cast {
        kind: sqlparser::ast::CastKind::Cast,
        expr: Box::new(d),
        data_type: DataType::Date,
        format: None,
    };
    Some(call("DATE_ADD", vec![day_lit, n_signed, date_cast]))
}

fn rewrite_datediff(f: &mut Function) -> Option<Expr> {
    // Spark: DATEDIFF(a, b) -> Trino: DATE_DIFF('day', CAST(b AS DATE), CAST(a AS DATE))
    let mut args = take_positional_args(f, 2)?;
    let b = args.remove(1);
    let a = args.remove(0);
    let day_lit = Expr::Value(Value::SingleQuotedString("day".to_string()));
    let cast_date = |e: Expr| Expr::Cast {
        kind: sqlparser::ast::CastKind::Cast,
        expr: Box::new(e),
        data_type: DataType::Date,
        format: None,
    };
    Some(call(
        "DATE_DIFF",
        vec![day_lit, cast_date(b), cast_date(a)],
    ))
}

fn rewrite_to_date(f: &mut Function) -> Option<Expr> {
    // Spark: TO_DATE(s)         -> Trino: DATE(CAST(s AS TIMESTAMP))
    // Spark: TO_DATE(s, fmt)    -> leave alone (Trino's DATE_PARSE signature differs
    //                              and user-supplied formats are not always
    //                              round-trippable Hive→Trino).
    let args = take_all_positional_args(f)?;
    if args.len() != 1 {
        return Some(call_with("TO_DATE", args));
    }
    let s = args.into_iter().next().unwrap();
    let ts_cast = Expr::Cast {
        kind: sqlparser::ast::CastKind::Cast,
        expr: Box::new(s),
        data_type: DataType::Timestamp(None, sqlparser::ast::TimezoneInfo::None),
        format: None,
    };
    Some(call("DATE", vec![ts_cast]))
}

// ---------- helpers (duplicated from rewrite/functions.rs to keep the passes
// ---------- independently testable / removable) ----------

fn function_name_lower(f: &Function) -> String {
    f.name
        .0
        .last()
        .map(|i| i.value.to_lowercase())
        .unwrap_or_default()
}

fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Function(Function {
        name: ObjectName(vec![Ident::new(name)]),
        parameters: FunctionArguments::None,
        args: FunctionArguments::List(FunctionArgumentList {
            duplicate_treatment: None,
            args: args
                .into_iter()
                .map(|e| FunctionArg::Unnamed(FunctionArgExpr::Expr(e)))
                .collect(),
            clauses: vec![],
        }),
        filter: None,
        null_treatment: None,
        over: None,
        within_group: vec![],
    })
}

fn call_with(name: &str, args: Vec<Expr>) -> Expr {
    call(name, args)
}

fn empty_arg_list() -> FunctionArgumentList {
    FunctionArgumentList {
        duplicate_treatment: None,
        args: vec![],
        clauses: vec![],
    }
}

fn dummy_expr() -> Expr {
    Expr::Value(Value::Null)
}

fn take_positional_args(f: &mut Function, n: usize) -> Option<Vec<Expr>> {
    let FunctionArguments::List(list) = &mut f.args else {
        return None;
    };
    if list.args.len() != n {
        return None;
    }
    let mut out = Vec::with_capacity(n);
    for a in std::mem::take(&mut list.args) {
        match a {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => out.push(e),
            other => {
                list.args.push(other);
                return None;
            }
        }
    }
    Some(out)
}

fn take_all_positional_args(f: &mut Function) -> Option<Vec<Expr>> {
    let FunctionArguments::List(list) = &mut f.args else {
        return None;
    };
    let mut out = Vec::with_capacity(list.args.len());
    for a in std::mem::take(&mut list.args) {
        match a {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => out.push(e),
            other => {
                list.args.push(other);
                return None;
            }
        }
    }
    Some(out)
}
