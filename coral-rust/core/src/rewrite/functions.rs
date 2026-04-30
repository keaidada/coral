// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Function / operator-level rewrites.
//!
//! Full port of the GaussDB -> Spark function mappings from
//! `coral-gaussdb`'s `ParseTreeBuilder.visitFunctionCall` — 30 rules:
//!
//! | GaussDB           | Spark                                             |
//! |-------------------|---------------------------------------------------|
//! | `NVL(a, b)`       | `COALESCE(a, b)`                                  |
//! | `NVL2(a, b, c)`   | `CASE WHEN a IS NOT NULL THEN b ELSE c END`       |
//! | `DECODE(...)`     | `CASE WHEN x = k THEN v ... [ELSE d] END`         |
//! | `SUBSTR`          | `SUBSTRING`                                       |
//! | `MOD(a, b)`       | `a % b`                                           |
//! | `SYSDATE`/`NOW`   | `CURRENT_TIMESTAMP`                               |
//! | `RANDOM`          | `RAND`                                            |
//! | `POSITION(a,b)`   | `INSTR(b, a)` (args swapped)                      |
//! | `BOOL_AND`        | `EVERY`                                           |
//! | `BOOL_OR`         | `SOME`                                            |
//! | `ARRAY_AGG(x)`    | `COLLECT_LIST(x)`                                 |
//! | `STRING_AGG(x,s)` | `CONCAT_WS(s, COLLECT_LIST(x))`                   |
//! | `TRUNC(d, 'MM')`  | `DATE_TRUNC('MM', d)` (arg swap, date only)       |
//! | `REGEXP_SUBSTR`   | `REGEXP_EXTRACT(s, p, 0)`                         |
//! | `GENERATE_SERIES` | `SEQUENCE`                                        |
//! | `TO_CHAR(d, fmt)` | `DATE_FORMAT(d, translated_fmt)` (date-ish only)  |
//! | `TO_DATE(s, fmt)` | `TO_DATE(s, translated_fmt)`                      |
//! | `TO_TIMESTAMP`    | `TO_TIMESTAMP(s, translated_fmt)`                 |
//! | `x::T`            | `CAST(x AS T)`                                    |
//! | `x ~ p`           | `x RLIKE p`                                       |
//! | `x ~* p`          | `LOWER(x) RLIKE LOWER(p)`                         |
//! | `x !~ p` / `!~*`  | `NOT (...)` of above                              |
//!
//! Non-GaussDB-specific functions (COALESCE, COUNT, SUM, ROW_NUMBER, …) are
//! left untouched — they pass through sqlparser-rs's Display as valid Spark SQL.

use sqlparser::ast::{
    BinaryOperator, CastKind, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments,
    Ident, ObjectName, UnaryOperator, Value, VisitorMut,
};
use std::ops::ControlFlow;

use crate::date_format::{contains_date_format_token, translate_pg_date_format};

pub struct FunctionRewriter;

impl VisitorMut for FunctionRewriter {
    type Break = std::convert::Infallible;

    fn post_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        // ---- 1) `::` cast -> CAST(x AS T)
        if let Expr::Cast { kind, .. } = expr {
            if *kind == CastKind::DoubleColon {
                *kind = CastKind::Cast;
            }
        }

        // ---- 2) PG regex operators -> Spark RLIKE equivalents
        if let Expr::BinaryOp { left, op, right } = expr {
            let new = match op {
                BinaryOperator::PGRegexMatch => Some(rlike(left, right, false)),
                BinaryOperator::PGRegexIMatch => Some(rlike(left, right, true)),
                BinaryOperator::PGRegexNotMatch => Some(negate(rlike(left, right, false))),
                BinaryOperator::PGRegexNotIMatch => Some(negate(rlike(left, right, true))),
                _ => None,
            };
            if let Some(new_expr) = new {
                *expr = new_expr;
                return ControlFlow::Continue(());
            }
        }

        // ---- 3) Function-level rewrites (post-order: args already rewritten)
        if let Expr::Function(f) = expr {
            if let Some(replacement) = rewrite_function(f) {
                *expr = replacement;
            }
        }

        ControlFlow::Continue(())
    }
}

/// Core of the function rewriter: dispatch on the lowercased function name
/// and return Some(new_expr) to replace the call, or None to leave untouched.
///
/// Mutates `f` in place for rename-only cases (SUBSTR -> SUBSTRING) to preserve
/// the function's filter/over/within_group clauses without re-boxing.
///
/// Decision flow:
///   1. Look up the function in `function_catalog::registry`. If it's not
///      there, fall through to pass-through — safer than silently rewriting
///      something we haven't vetted.
///   2. For Rename(new) → just change the ObjectName.
///   3. For CustomRewrite → dispatch to the appropriate helper below.
///   4. For Passthrough / UnsupportedBySpark → no-op (Spark will handle or
///      error out at runtime).
fn rewrite_function(f: &mut Function) -> Option<Expr> {
    let name = function_name_lower(f);

    // Drive from the registry when possible.
    if let Some(entry) = crate::function_catalog::lookup(&name) {
        match entry.disposition {
            crate::function_catalog::Disposition::Passthrough
            | crate::function_catalog::Disposition::UnsupportedBySpark => {
                return None;
            }
            crate::function_catalog::Disposition::Rename(new_name) => {
                f.name = ObjectName(vec![Ident::new(new_name)]);
                return None;
            }
            crate::function_catalog::Disposition::CustomRewrite => {
                // Fall through to the explicit dispatch below.
            }
        }
    }

    // CustomRewrite + legacy unregistered names: explicit dispatch.
    match name.as_str() {
        "nvl2" => rewrite_nvl2(f),
        "decode" => rewrite_decode(f),
        "mod" => rewrite_mod(f),
        "sysdate" | "now" => {
            // Both take no args in Spark; produce CURRENT_TIMESTAMP as a
            // function call (Spark accepts both with and without parens).
            f.name = ObjectName(vec![Ident::new("CURRENT_TIMESTAMP")]);
            f.args = FunctionArguments::None;
            None
        }
        "position" => rewrite_position(f),
        "string_agg" => rewrite_string_agg(f),
        "trunc" => rewrite_trunc(f),
        "regexp_substr" => rewrite_regexp_substr(f),
        "to_char" => rewrite_to_char(f),
        "to_date" | "to_timestamp" => {
            rewrite_date_format_arg(f);
            None
        }

        _ => None,
    }
}

// ---------- individual rewrites ----------

fn rewrite_nvl2(f: &mut Function) -> Option<Expr> {
    let mut args = take_positional_args(f, 3)?;
    let a = args.remove(0);
    let b = args.remove(0);
    let c = args.remove(0);
    Some(Expr::Case {
        operand: None,
        conditions: vec![Expr::IsNotNull(Box::new(a))],
        results: vec![b],
        else_result: Some(Box::new(c)),
    })
}

fn rewrite_decode(f: &mut Function) -> Option<Expr> {
    let args = take_all_positional_args(f)?;
    if args.len() < 3 {
        return None;
    }
    let mut iter = args.into_iter();
    let x = iter.next().unwrap();
    let mut conditions = vec![];
    let mut results = vec![];

    let rest: Vec<Expr> = iter.collect();
    let has_default = rest.len() % 2 == 1;
    let pair_count = rest.len() / 2;
    let mut it = rest.into_iter();
    for _ in 0..pair_count {
        let k = it.next().unwrap();
        let v = it.next().unwrap();
        conditions.push(Expr::BinaryOp {
            left: Box::new(x.clone()),
            op: BinaryOperator::Eq,
            right: Box::new(k),
        });
        results.push(v);
    }
    let else_result = if has_default {
        Some(Box::new(it.next().unwrap()))
    } else {
        None
    };

    Some(Expr::Case {
        operand: None,
        conditions,
        results,
        else_result,
    })
}

fn rewrite_mod(f: &mut Function) -> Option<Expr> {
    let mut args = take_positional_args(f, 2)?;
    let b = args.remove(1);
    let a = args.remove(0);
    Some(Expr::BinaryOp {
        left: Box::new(a),
        op: BinaryOperator::Modulo,
        right: Box::new(b),
    })
}

fn rewrite_position(f: &mut Function) -> Option<Expr> {
    // POSITION(a, b) -> INSTR(b, a)  (arg swap)
    let mut args = take_positional_args(f, 2)?;
    let b = args.remove(1);
    let a = args.remove(0);
    Some(call("INSTR", vec![b, a]))
}

fn rewrite_string_agg(f: &mut Function) -> Option<Expr> {
    // STRING_AGG(x, sep) -> CONCAT_WS(sep, COLLECT_LIST(x))
    let mut args = take_positional_args(f, 2)?;
    let sep = args.remove(1);
    let x = args.remove(0);
    Some(call("CONCAT_WS", vec![sep, call("COLLECT_LIST", vec![x])]))
}

fn rewrite_trunc(f: &mut Function) -> Option<Expr> {
    // TRUNC(date, unit-literal) -> DATE_TRUNC(unit, date)  (arg swap)
    // TRUNC(numeric, digits) passes through (same semantics in Spark).
    let args = take_all_positional_args(f)?;
    if args.len() != 2 {
        return Some(call("TRUNC", args));
    }
    // Peek at arg[1]: if it's a string literal, do the date_trunc rewrite.
    if let Expr::Value(Value::SingleQuotedString(_)) = &args[1] {
        let mut it = args.into_iter();
        let d = it.next().unwrap();
        let unit = it.next().unwrap();
        Some(call("DATE_TRUNC", vec![unit, d]))
    } else {
        Some(call("TRUNC", args))
    }
}

fn rewrite_regexp_substr(f: &mut Function) -> Option<Expr> {
    // REGEXP_SUBSTR(s, p [, pos [, occurrence]]) -> REGEXP_EXTRACT(s, p, 0).
    // We drop pos/occurrence because Spark's regexp_extract takes a
    // capture-group index there. A richer translation is future work.
    let args = take_all_positional_args(f)?;
    if args.len() >= 2 {
        let mut it = args.into_iter();
        let s = it.next().unwrap();
        let p = it.next().unwrap();
        let zero = Expr::Value(Value::Number("0".into(), false));
        Some(call("REGEXP_EXTRACT", vec![s, p, zero]))
    } else {
        Some(call("REGEXP_SUBSTR", args))
    }
}

fn rewrite_to_char(f: &mut Function) -> Option<Expr> {
    // TO_CHAR(d, 'YYYY-MM-DD') -> DATE_FORMAT(d, 'yyyy-MM-dd')
    // TO_CHAR(n, 'fm999.99') passes through — Spark's to_char handles numerics.
    let FunctionArguments::List(list) = &mut f.args else {
        return None;
    };
    if list.args.len() != 2 {
        return None;
    }
    // Inspect arg[1] non-destructively.
    let fmt_is_datish = matches!(
        &list.args[1],
        FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(
            Value::SingleQuotedString(s)
        ))) if contains_date_format_token(s)
    );
    if !fmt_is_datish {
        return None;
    }
    // Destructure and rebuild as DATE_FORMAT with the translated literal.
    let args = std::mem::take(&mut list.args);
    let mut exprs: Vec<Expr> = args
        .into_iter()
        .filter_map(|a| match a {
            FunctionArg::Unnamed(FunctionArgExpr::Expr(e)) => Some(e),
            _ => None,
        })
        .collect();
    if exprs.len() != 2 {
        return None;
    }
    let fmt_expr = exprs.remove(1);
    let d = exprs.remove(0);

    let new_fmt = if let Expr::Value(Value::SingleQuotedString(s)) = fmt_expr {
        Expr::Value(Value::SingleQuotedString(translate_pg_date_format(&s)))
    } else {
        fmt_expr
    };
    Some(call("DATE_FORMAT", vec![d, new_fmt]))
}

/// In-place rewrite of the 2nd arg to TO_DATE / TO_TIMESTAMP when it's a
/// string literal containing date tokens. Keeps the function name.
fn rewrite_date_format_arg(f: &mut Function) {
    let FunctionArguments::List(list) = &mut f.args else {
        return;
    };
    if list.args.len() < 2 {
        return;
    }
    // Mutate arg[1] in place.
    if let FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(Value::SingleQuotedString(s)))) =
        &mut list.args[1]
    {
        if contains_date_format_token(s) {
            *s = translate_pg_date_format(s);
        }
    }
}

// ---------- small helpers ----------

fn rlike(left: &Expr, right: &Expr, case_insensitive: bool) -> Expr {
    let (l, r) = if case_insensitive {
        (
            call("LOWER", vec![left.clone()]),
            call("LOWER", vec![right.clone()]),
        )
    } else {
        (left.clone(), right.clone())
    };
    Expr::BinaryOp {
        left: Box::new(l),
        op: BinaryOperator::Custom("RLIKE".into()),
        right: Box::new(r),
    }
}

fn negate(inner: Expr) -> Expr {
    Expr::UnaryOp {
        op: UnaryOperator::Not,
        expr: Box::new(Expr::Nested(Box::new(inner))),
    }
}

fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Function(Function {
        name: ObjectName(vec![Ident::new(name)]),
        parameters: FunctionArguments::None,
        args: FunctionArguments::List(sqlparser::ast::FunctionArgumentList {
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

fn function_name_lower(f: &Function) -> String {
    f.name
        .0
        .last()
        .map(|i| i.value.to_lowercase())
        .unwrap_or_default()
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
