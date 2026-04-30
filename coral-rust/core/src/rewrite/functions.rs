// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Function / operator-level rewrites.
//!
//! Mirrors the entries in `StaticHiveFunctionRegistry` + GaussDB-specific
//! handling inside `coral-gaussdb`'s ParseTreeBuilder:
//!
//! | GaussDB input           | Spark output                          |
//! |-------------------------|---------------------------------------|
//! | `NVL(a, b)`             | `COALESCE(a, b)`                      |
//! | `NVL2(a, b, c)`         | `CASE WHEN a IS NOT NULL THEN b ELSE c END` |
//! | `DECODE(x, k1, v1, …, d)` | `CASE WHEN x = k1 THEN v1 … ELSE d END` |
//! | `SUBSTR(x, s, l)`       | `SUBSTRING(x, s, l)`                  |
//! | `MOD(a, b)`             | `a % b`                               |
//! | `x::INT`                | `CAST(x AS INT)`                      |
//! | `x ~ p`                 | `x RLIKE p`                           |
//! | `x ~* p`                | `LOWER(x) RLIKE LOWER(p)`             |
//! | `x !~ p` / `x !~* p`    | `NOT` of the above                    |
//! | `SYSDATE` / `NOW()`     | `CURRENT_TIMESTAMP`                   |
//! | `RANDOM()`              | `RAND()`                              |

use sqlparser::ast::{
    BinaryOperator, CastKind, Expr, Function, FunctionArg, FunctionArgExpr, FunctionArguments,
    Ident, ObjectName, UnaryOperator, VisitorMut,
};
use std::ops::ControlFlow;

pub struct FunctionRewriter;

impl VisitorMut for FunctionRewriter {
    type Break = std::convert::Infallible;

    fn post_visit_expr(&mut self, expr: &mut Expr) -> ControlFlow<Self::Break> {
        // Post-order: children have already been rewritten by the time we see
        // the parent. That means when we replace `NVL(NVL(x, y), z)` we see the
        // inner NVL first; when we later visit the outer NVL, its args are
        // already `COALESCE(x, y)`, and the outer becomes `COALESCE(COALESCE(x, y), z)`.

        // 1) `::` cast -> CAST(x AS T)
        if let Expr::Cast { kind, .. } = expr {
            if *kind == CastKind::DoubleColon {
                *kind = CastKind::Cast;
            }
        }

        // 2) PG regex operators -> RLIKE / NOT RLIKE with case-folding for IMatch
        if let Expr::BinaryOp { left, op, right } = expr {
            let new = match op {
                BinaryOperator::PGRegexMatch => {
                    // x ~ p  ->  x RLIKE p  (Custom("RLIKE") so the Display
                    // literally writes "RLIKE")
                    Some(Expr::BinaryOp {
                        left: left.clone(),
                        op: BinaryOperator::Custom("RLIKE".into()),
                        right: right.clone(),
                    })
                }
                BinaryOperator::PGRegexIMatch => {
                    // x ~* p  ->  LOWER(x) RLIKE LOWER(p)
                    Some(Expr::BinaryOp {
                        left: Box::new(wrap_unary_fn("LOWER", (**left).clone())),
                        op: BinaryOperator::Custom("RLIKE".into()),
                        right: Box::new(wrap_unary_fn("LOWER", (**right).clone())),
                    })
                }
                BinaryOperator::PGRegexNotMatch => Some(Expr::UnaryOp {
                    op: UnaryOperator::Not,
                    expr: Box::new(Expr::Nested(Box::new(Expr::BinaryOp {
                        left: left.clone(),
                        op: BinaryOperator::Custom("RLIKE".into()),
                        right: right.clone(),
                    }))),
                }),
                BinaryOperator::PGRegexNotIMatch => Some(Expr::UnaryOp {
                    op: UnaryOperator::Not,
                    expr: Box::new(Expr::Nested(Box::new(Expr::BinaryOp {
                        left: Box::new(wrap_unary_fn("LOWER", (**left).clone())),
                        op: BinaryOperator::Custom("RLIKE".into()),
                        right: Box::new(wrap_unary_fn("LOWER", (**right).clone())),
                    }))),
                }),
                _ => None,
            };
            if let Some(new_expr) = new {
                *expr = new_expr;
                return ControlFlow::Continue(());
            }
        }

        // 3) Function-level rewrites
        if let Expr::Function(f) = expr {
            let name_lower = function_name_lower(f);
            match name_lower.as_str() {
                "nvl" => {
                    if let Some(args) = take_positional_args(f, 2) {
                        *expr = Expr::Function(Function {
                            name: ObjectName(vec![Ident::new("COALESCE")]),
                            parameters: FunctionArguments::None,
                            args: build_positional_args(args),
                            filter: None,
                            null_treatment: None,
                            over: None,
                            within_group: vec![],
                        });
                    }
                }
                "nvl2" => {
                    // NVL2(a, b, c) -> CASE WHEN a IS NOT NULL THEN b ELSE c END
                    if let Some(mut args) = take_positional_args(f, 3) {
                        let a = args.remove(0);
                        let b = args.remove(0);
                        let c = args.remove(0);
                        *expr = Expr::Case {
                            operand: None,
                            conditions: vec![Expr::IsNotNull(Box::new(a))],
                            results: vec![b],
                            else_result: Some(Box::new(c)),
                        };
                    }
                }
                "decode" => {
                    // DECODE(x, k1, v1, k2, v2, [default])
                    // -> CASE WHEN x = k1 THEN v1 WHEN x = k2 THEN v2 [ELSE default] END
                    if let Some(args) = take_all_positional_args(f) {
                        if args.len() >= 3 {
                            let mut iter = args.into_iter();
                            let x = iter.next().unwrap();
                            let mut conditions = vec![];
                            let mut results = vec![];
                            let mut else_result: Option<Expr> = None;

                            let rest: Vec<Expr> = iter.collect();
                            let pair_count = rest.len() / 2;
                            let has_default = rest.len() % 2 == 1;
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
                            if has_default {
                                else_result = Some(it.next().unwrap());
                            }

                            *expr = Expr::Case {
                                operand: None,
                                conditions,
                                results,
                                else_result: else_result.map(Box::new),
                            };
                        }
                    }
                }
                "substr" => {
                    // SUBSTR(x, s, l) -> SUBSTRING(x, s, l)
                    // Same arg layout, just rename. Preserve window/filter etc.
                    f.name = ObjectName(vec![Ident::new("SUBSTRING")]);
                }
                "mod" => {
                    // MOD(a, b) -> a % b
                    if let Some(mut args) = take_positional_args(f, 2) {
                        let b = args.remove(1);
                        let a = args.remove(0);
                        *expr = Expr::BinaryOp {
                            left: Box::new(a),
                            op: BinaryOperator::Modulo,
                            right: Box::new(b),
                        };
                    }
                }
                "sysdate" => {
                    // GaussDB `SYSDATE` is a no-paren identifier; if parser does
                    // pick it up as a Function it's still safe to map to
                    // CURRENT_TIMESTAMP.
                    f.name = ObjectName(vec![Ident::new("CURRENT_TIMESTAMP")]);
                    f.args = FunctionArguments::None;
                }
                "now" => {
                    f.name = ObjectName(vec![Ident::new("CURRENT_TIMESTAMP")]);
                    f.args = FunctionArguments::None;
                }
                "random" => {
                    f.name = ObjectName(vec![Ident::new("RAND")]);
                }
                _ => {}
            }
        }

        ControlFlow::Continue(())
    }
}

/// Extract the lowercased leaf name of a function reference (e.g. `PUBLIC.NVL`
/// -> `nvl`). Empty string if the function has no name parts (shouldn't happen
/// for well-formed input, but we defend against it).
fn function_name_lower(f: &Function) -> String {
    f.name
        .0
        .last()
        .map(|ident| ident.value.to_lowercase())
        .unwrap_or_default()
}

/// Take exactly `n` positional (`Unnamed(FunctionArgExpr::Expr(_))`) arguments
/// out of a function. Returns None if the shape doesn't match — the caller
/// should then leave the function untouched.
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
                // Put it back and bail out — we can't rewrite non-positional
                // forms (named args, wildcards, qualified wildcards) without
                // knowing user intent. Return None and restore.
                list.args.push(other);
                return None;
            }
        }
    }
    Some(out)
}

/// Take every positional argument the function has. Returns None on any
/// non-positional arg.
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

/// Wrap expressions into `FunctionArguments::List` of unnamed positional args.
fn build_positional_args(exprs: Vec<Expr>) -> FunctionArguments {
    FunctionArguments::List(sqlparser::ast::FunctionArgumentList {
        duplicate_treatment: None,
        args: exprs
            .into_iter()
            .map(|e| FunctionArg::Unnamed(FunctionArgExpr::Expr(e)))
            .collect(),
        clauses: vec![],
    })
}

/// Build a call `NAME(inner)` — used to wrap expressions in `LOWER(...)`.
fn wrap_unary_fn(name: &str, inner: Expr) -> Expr {
    Expr::Function(Function {
        name: ObjectName(vec![Ident::new(name)]),
        parameters: FunctionArguments::None,
        args: build_positional_args(vec![inner]),
        filter: None,
        null_treatment: None,
        over: None,
        within_group: vec![],
    })
}
