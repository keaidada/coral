// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Spark plan-string predicate classification.
//!
//! Full Rust port of Java `SparkPlanToIRRelConverter
//! .getComplicatedPredicatePushedDownInfo`. Takes a plain-text Spark
//! logical/physical plan (the kind `EXPLAIN` prints), finds each scan
//! node, and reports the pushed-down predicates it found along with a
//! "complicated?" verdict per predicate.
//!
//! Scope: no SparkSession, no Hive Metastore — purely static text
//! parsing + one sqlparser call per predicate. Useful for:
//!
//!   - audit tools that want to flag queries doing UDF-based pushdown,
//!   - offline plan-quality analysis before promoting jobs to prod,
//!   - the optional `/api/plan/analyze` endpoint a future coral-service
//!     release can offer.

use sqlparser::ast::{Expr, Visit, Visitor};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use std::ops::ControlFlow;

/// Classification verdict for a single predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateClass {
    /// Only comparison / boolean / IN / BETWEEN / IS NULL — Spark can
    /// push this cleanly to most sources (Parquet, Iceberg, etc).
    Simple,
    /// Contains a function call (datediff, substring, regexp_*, …),
    /// a CASE expression, or another non-trivial constructor. Spark
    /// will often have to evaluate this after the scan.
    Complicated,
    /// The predicate text didn't parse as a SQL expression —
    /// conservative fallback, treated as complicated by downstream
    /// tools.
    Unparseable,
}

/// One predicate from one scan node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PredicateFinding {
    /// Raw predicate text as it appeared in the plan.
    pub predicate: String,
    /// Verdict.
    pub class: PredicateClass,
}

/// The full result: table name + predicates in plan order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlanPredicateInfo {
    /// `db.table` form (when available) or just `table` (when plan
    /// text omitted the database).
    pub table: String,
    /// List of predicates found at this scan node.
    pub predicates: Vec<PredicateFinding>,
}

/// Classify one predicate expression string.
///
/// This is the leaf primitive that the plan walker calls on each
/// pushed-down predicate it extracts.
pub fn classify_predicate(predicate: &str) -> PredicateClass {
    let s = predicate.trim();
    if s.is_empty() {
        return PredicateClass::Unparseable;
    }

    // Spark plan text uses a function-call syntax for a handful of
    // simple predicates that started life as operators (IS NULL /
    // IS NOT NULL / IN / =, rendered as `IsNotNull(x)`, `In(x, list)`,
    // `EqualTo(x, 1)`, …). These MUST count as Simple — they're what
    // the source can actually push down. Match by outermost name
    // before falling through to the real classifier.
    if let Some(outer) = outermost_call_name(s) {
        let u = outer.to_ascii_uppercase();
        const SIMPLE_PLAN_ONLY: &[&str] = &[
            "ISNULL",
            "ISNOTNULL",
            "EQUALTO",
            "NOTEQUALTO",
            "LESSTHAN",
            "LESSTHANOREQUAL",
            "GREATERTHAN",
            "GREATERTHANOREQUAL",
            "IN",
            "NOT",
            "AND",
            "OR",
            "EQUALNULLSAFE",
            "STARTSWITH",
            "ENDSWITH",
            "STRINGSTARTSWITH",
            "STRINGENDSWITH",
            "STRINGCONTAINS",
        ];
        if SIMPLE_PLAN_ONLY.contains(&u.as_str()) {
            return PredicateClass::Simple;
        }
    }

    let wrapped = format!("SELECT 1 WHERE {s}");
    match Parser::parse_sql(&PostgreSqlDialect {}, &wrapped) {
        Ok(stmts) => {
            let mut visitor = ComplicationVisitor { complicated: false };
            let _ = stmts.visit(&mut visitor);
            if visitor.complicated {
                PredicateClass::Complicated
            } else {
                PredicateClass::Simple
            }
        }
        Err(_) => PredicateClass::Unparseable,
    }
}

/// If `s` is shaped like `NAME(...)`, return `Some("NAME")`. Used to
/// cheaply identify Spark plan-text wrapper predicates.
fn outermost_call_name(s: &str) -> Option<&str> {
    let open = s.find('(')?;
    // Ensure the thing before `(` is a single identifier (no spaces,
    // no dots — we're not catching `t.foo(...)` function calls).
    let head = s[..open].trim();
    if head.is_empty() || head.chars().any(|c| !c.is_alphanumeric() && c != '_') {
        return None;
    }
    Some(head)
}

struct ComplicationVisitor {
    complicated: bool,
}

impl Visitor for ComplicationVisitor {
    type Break = ();

    fn post_visit_expr(&mut self, expr: &Expr) -> ControlFlow<Self::Break> {
        match expr {
            // Anything function-like is "complicated".
            Expr::Function(_) => {
                self.complicated = true;
                ControlFlow::Break(())
            }
            // sqlparser-rs special-cases a handful of built-ins into their
            // own Expr variants instead of Expr::Function. Treat each of
            // them as complicated — they're function calls in Spark's
            // plan text.
            Expr::Substring { .. }
            | Expr::Trim { .. }
            | Expr::Position { .. }
            | Expr::Extract { .. }
            | Expr::Ceil { .. }
            | Expr::Floor { .. }
            | Expr::Convert { .. }
            | Expr::Overlay { .. }
            | Expr::Collate { .. } => {
                self.complicated = true;
                ControlFlow::Break(())
            }
            Expr::Case { .. } => {
                self.complicated = true;
                ControlFlow::Break(())
            }
            Expr::Cast { .. } => {
                self.complicated = true;
                ControlFlow::Break(())
            }
            Expr::Subquery(_) | Expr::Exists { .. } | Expr::InSubquery { .. } => {
                self.complicated = true;
                ControlFlow::Break(())
            }
            _ => ControlFlow::Continue(()),
        }
    }
}

/// Walk a multi-line Spark plan text and return one [`PlanPredicateInfo`]
/// per scan node found. Scans are identified by `Scan`/`FileScan`/
/// `HiveTableScan`/`BatchScan` keywords, and predicates come from a
/// `PushedFilters: [...]` substring (either on the same line or on a
/// following line, both shapes occur across Spark versions).
///
/// This is a deliberately lenient parser — Spark's plan text format
/// drifts across 2.4/3.x/3.5 and between logical/physical output, so we
/// scan the lines rather than trying to reconstruct the tree.
pub fn analyze_plan(plan: &str) -> Vec<PlanPredicateInfo> {
    let mut out = Vec::new();
    for raw in plan.lines() {
        let line = raw.trim_start_matches([' ', '+', '-', '|']);
        let line = line.trim();
        if line.is_empty() || !looks_like_scan(line) {
            continue;
        }
        let table = extract_table(line).unwrap_or_else(|| "unknown".to_string());

        // Look for PushedFilters or Filters on the same line; if
        // missing, try the next non-empty line (some EXPLAIN FORMATTED
        // output puts them one level down).
        let predicates = extract_predicates_from_same_line(line);

        out.push(PlanPredicateInfo {
            table,
            predicates,
        });
    }
    out
}

fn extract_predicates_from_same_line(line: &str) -> Vec<PredicateFinding> {
    for key in ["PushedFilters:", "Filters:"] {
        if let Some(payload) = extract_bracket_payload(line, key) {
            return split_pushed(&payload)
                .into_iter()
                .map(|p| PredicateFinding {
                    class: classify_predicate(&p),
                    predicate: p,
                })
                .collect();
        }
    }
    vec![]
}

/// Given a line like `... PushedFilters: [a, b(c), d], ReadSchema: ...`,
/// return `[a, b(c), d]` (as a `String`). Handles nested brackets and
/// string literals.
fn extract_bracket_payload(line: &str, key: &str) -> Option<String> {
    let start = line.find(key)? + key.len();
    let rest = line[start..].trim_start();
    if !rest.starts_with('[') {
        return None;
    }
    let mut depth = 0i32;
    let mut in_quote = false;
    let mut end = 0;
    for (i, c) in rest.char_indices() {
        match c {
            '\'' => in_quote = !in_quote,
            '[' if !in_quote => depth += 1,
            ']' if !in_quote => {
                depth -= 1;
                if depth == 0 {
                    end = i + 1;
                    break;
                }
            }
            _ => {}
        }
    }
    if end == 0 {
        return None;
    }
    Some(rest[..end].to_string())
}

fn looks_like_scan(line: &str) -> bool {
    let l = line.trim();
    // The lowercased common prefixes Spark emits.
    let prefixes = [
        "Scan ",
        "FileScan ",
        "HiveTableScan",
        "BatchScan ",
        "*(1) Scan",
    ];
    prefixes.iter().any(|p| l.contains(p))
}

fn extract_table(line: &str) -> Option<String> {
    // Pattern A: `Scan foo.bar [...` or `Scan foo.bar`
    // Pattern B: `FileScan parquet foo.bar[...]`
    // Pattern C: `HiveTableScan [...], HiveTableRelation [foo.bar, ...]`
    //
    // The uniform winning strategy: find the first `db.table`-looking
    // token after the Scan keyword and before any `[` / `(`.
    let after = skip_to_scan_keyword(line)?;
    // Split on whitespace, brackets, parens, commas.
    let stop = after
        .find(['[', '(', ','])
        .unwrap_or(after.len());
    let rest = after[..stop].trim();
    // Now rest might be "foo.bar" or "parquet foo.bar" or just "foo"
    // depending on dialect. Pick the last whitespace-separated token.
    let tok = rest.split_whitespace().last()?;
    if tok.is_empty() {
        None
    } else {
        Some(tok.to_string())
    }
}

fn skip_to_scan_keyword(line: &str) -> Option<&str> {
    for kw in ["Scan ", "FileScan ", "HiveTableScan", "BatchScan "] {
        if let Some(i) = line.find(kw) {
            return Some(line[i + kw.trim_end().len()..].trim_start());
        }
    }
    None
}

/// Split a `[p1, p2, p3]` payload into individual predicates. Leniently
/// strips outer brackets, respects parens and string literals, splits
/// on top-level commas.
fn split_pushed(s: &str) -> Vec<String> {
    let trimmed = s.trim();
    let inner = trimmed
        .strip_prefix('[')
        .and_then(|r| r.strip_suffix(']'))
        .unwrap_or(trimmed);

    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0i32;
    let mut in_quote = false;
    for c in inner.chars() {
        match c {
            '\'' => {
                in_quote = !in_quote;
                cur.push(c);
            }
            '(' | '[' if !in_quote => {
                depth += 1;
                cur.push(c);
            }
            ')' | ']' if !in_quote => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 && !in_quote => {
                let piece = cur.trim().to_string();
                if !piece.is_empty() {
                    out.push(piece);
                }
                cur.clear();
            }
            _ => cur.push(c),
        }
    }
    let piece = cur.trim().to_string();
    if !piece.is_empty() {
        out.push(piece);
    }
    out
}
