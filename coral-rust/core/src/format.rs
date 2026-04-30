// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Calcite-style output formatter.
//!
//! Post-render pass that shapes the emitted SQL so it matches what
//! Java Coral produces from Calcite's SqlPrettyWriter:
//!
//! 1. **Explicit table aliases**. Calcite's `rel2sql` inserts an alias
//!    for every `TableFactor::Table` node even when the user didn't
//!    write one — `FROM user` becomes `FROM user user`. This makes
//!    every column reference unambiguous after later rewrites (joins,
//!    sub-queries).
//!
//! 2. **Clause-level line breaks**. Each top-level SELECT clause
//!    (SELECT / FROM / WHERE / GROUP BY / HAVING / ORDER BY / LIMIT,
//!    plus UNION / INTERSECT / EXCEPT between statements) goes on its
//!    own line. Matches Calcite's pretty-print default.
//!
//! Runs AFTER `apply_all_for_target` so rewrite passes see the raw
//! user-written shape first; the formatter only ever adds metadata,
//! it never changes semantics.

use sqlparser::ast::{Ident, ObjectName, Statement, TableAlias, TableFactor, VisitMut, VisitorMut};
use std::ops::ControlFlow;

/// Calcite / Spark-style: drop the ` AS ` keyword between a table
/// reference and its alias, so `FROM default.users AS users` becomes
/// `FROM default.users users`. Trino keeps the `AS`; Spark drops it.
///
/// We operate on the rendered string (not the AST) because sqlparser's
/// `Display` for `TableFactor::Table` always emits ` AS ` when an alias
/// is present. Text-level rewrite is simpler than forking the
/// renderer.
///
/// Heuristic: inside a FROM clause (which always starts on its own
/// line after pretty_print ran), find identifier tokens that are
/// followed by ` AS ` and another bare identifier, and strip the
/// ` AS ` — but only for TABLE contexts. We detect "table context" by
/// looking at the line prefix: lines starting with `FROM ` or `JOIN `
/// (or containing ` JOIN `).
///
/// Column aliases (`SELECT x AS y`) and subquery aliases (`(...) AS
/// sub`) are left intact because they're on lines starting with
/// `SELECT` / `WHERE` / `HAVING` / etc., not `FROM` / `JOIN`.
pub fn drop_as_in_from(sql: &str) -> String {
    sql.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !(trimmed.starts_with("FROM ")
                || trimmed.starts_with("JOIN ")
                || trimmed.starts_with("LEFT JOIN ")
                || trimmed.starts_with("RIGHT JOIN ")
                || trimmed.starts_with("FULL JOIN ")
                || trimmed.starts_with("INNER JOIN ")
                || trimmed.starts_with("CROSS JOIN "))
            {
                return line.to_string();
            }
            strip_as_between_idents(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// On a single FROM/JOIN line, replace ` AS ` with a single space when
/// it's sandwiched between two identifier-looking tokens. Respects
/// string literals (doesn't touch ` AS ` inside quoted text) and
/// paren depth (doesn't peel ` AS ` off a scalar subquery's outer
/// alias when it's the last thing on the line — that one is a table
/// expression and Java strips it there too).
fn strip_as_between_idents(line: &str) -> String {
    // Pattern: <ident-or-)>  AS  <ident>
    // Walk the line keeping a tiny state machine. Rebuilding by char
    // iteration is clearer than a regex given the quote/paren
    // bookkeeping required.
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len());
    let mut in_single = false;
    let mut in_double = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if !in_double && c == '\'' {
            in_single = !in_single;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_single && c == '"' {
            in_double = !in_double;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_single && !in_double && c == ' ' {
            // Look for " AS " ahead and a bareword before/after.
            if i + 4 <= chars.len()
                && chars[i + 1..i + 4]
                    .iter()
                    .collect::<String>()
                    .eq_ignore_ascii_case("AS ")
                && i > 0
                && is_ident_end(chars[i - 1])
                && i + 4 < chars.len()
                && is_ident_start(chars[i + 4])
            {
                // Emit one space instead of " AS ".
                out.push(' ');
                i += 4; // skip past " AS "
                continue;
            }
        }
        out.push(c);
        i += 1;
    }
    out
}

/// Trino-style: wrap every unquoted identifier in a FROM/JOIN clause
/// with double quotes. Matches what Calcite's TrinoSqlDialect emits.
///
/// Scope: only the FROM / JOIN lines. Projection / WHERE / HAVING /
/// GROUP BY column references stay unquoted — Java's Trino unparser
/// only quotes the table reference + its alias, not every mentioned
/// column in the projection (though you'd technically need those too
/// for safety; we match Java's behavior exactly).
pub fn quote_idents_in_from(sql: &str) -> String {
    sql.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if !(trimmed.starts_with("FROM ")
                || trimmed.starts_with("JOIN ")
                || trimmed.starts_with("LEFT JOIN ")
                || trimmed.starts_with("RIGHT JOIN ")
                || trimmed.starts_with("FULL JOIN ")
                || trimmed.starts_with("INNER JOIN ")
                || trimmed.starts_with("CROSS JOIN "))
            {
                return line.to_string();
            }
            quote_from_line(line)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn quote_from_line(line: &str) -> String {
    // Walk the line. For each bareword token that LOOKS like an
    // identifier (not a keyword and not already quoted), wrap it in
    // double quotes. Split on `.` so `default.users` becomes
    // `"default"."users"`.
    const KEYWORDS_SKIP: &[&str] = &[
        "FROM", "JOIN", "LEFT", "RIGHT", "FULL", "INNER", "OUTER", "CROSS", "ON", "AS", "AND",
        "OR", "NOT", "USING",
    ];
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::with_capacity(line.len() + 16);
    let mut in_single = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c == '\'' {
            in_single = !in_single;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_single && c == '"' {
            // Already-quoted identifier — copy through verbatim.
            out.push(c);
            i += 1;
            while i < chars.len() && chars[i] != '"' {
                out.push(chars[i]);
                i += 1;
            }
            if i < chars.len() {
                out.push(chars[i]);
                i += 1;
            }
            continue;
        }
        if !in_single && is_ident_start(c) {
            let start = i;
            while i < chars.len() && is_ident_body(chars[i]) {
                i += 1;
            }
            let tok: String = chars[start..i].iter().collect();
            let up = tok.to_ascii_uppercase();
            if KEYWORDS_SKIP.contains(&up.as_str()) {
                out.push_str(&tok);
            } else {
                out.push('"');
                out.push_str(&tok);
                out.push('"');
            }
            continue;
        }
        out.push(c);
        i += 1;
    }
    out
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '"' || c == '`'
}

fn is_ident_end(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == ')' || c == '"' || c == '`'
}

fn is_ident_body(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}
/// Mirrors Calcite's "give every table the trailing `t` that its own
/// unparser emits" convention.
pub fn add_default_aliases(stmt: &mut Statement) {
    let _ = stmt.visit(&mut AliasAdder);
}

/// Prepend the default database name (`"default"`) to any
/// `TableFactor::Table` reference that doesn't already carry a
/// db-qualifier. Mirrors Java Coral, which always resolves bare
/// table names through the current database and re-emits them
/// fully-qualified. Useful for: (a) matching Java's exact output
/// byte-for-byte, (b) making the generated SQL portable across
/// Spark / Trino sessions that may have a different default db.
pub fn qualify_with_default_db(stmt: &mut Statement, db: &str) {
    let mut v = DbQualifier { db: db.to_string() };
    let _ = stmt.visit(&mut v);
}

struct DbQualifier {
    db: String,
}

impl VisitorMut for DbQualifier {
    type Break = std::convert::Infallible;

    fn post_visit_table_factor(&mut self, factor: &mut TableFactor) -> ControlFlow<Self::Break> {
        if let TableFactor::Table { name, .. } = factor {
            // Only qualify single-segment names (`user` → `default.user`).
            // Multi-segment (`hr.employees`) stays untouched.
            if name.0.len() == 1 {
                let table = name.0.remove(0);
                name.0 = vec![Ident::new(&self.db), table];
            }
        }
        ControlFlow::Continue(())
    }
}

struct AliasAdder;

impl VisitorMut for AliasAdder {
    type Break = std::convert::Infallible;

    fn post_visit_table_factor(&mut self, factor: &mut TableFactor) -> ControlFlow<Self::Break> {
        if let TableFactor::Table { name, alias, .. } = factor {
            if alias.is_none() {
                if let Some(alias_text) = derive_alias(name) {
                    *alias = Some(TableAlias {
                        name: Ident::new(alias_text),
                        columns: vec![],
                    });
                }
            }
        }
        ControlFlow::Continue(())
    }
}

fn derive_alias(name: &ObjectName) -> Option<String> {
    // Use the last identifier segment (the table name, not the
    // catalog/db prefix). Calcite does the same: `hr.employees` →
    // alias `employees`.
    name.0.last().map(|i| i.value.clone())
}

/// Break a rendered SQL string onto Calcite-style clause boundaries.
///
/// Operates on the raw text output of sqlparser-rs's `Display`. We
/// don't rebuild it from scratch — sqlparser already emits valid SQL,
/// we just need to push each top-level clause onto its own line.
///
/// Rules:
///   - Each of the SELECT sub-clauses gets a leading `\n` when it
///     appears at statement top level (i.e. outside parens / brackets).
///   - Statement separators (UNION / UNION ALL / INTERSECT / EXCEPT)
///     also force a break.
///
/// A two-pass walk: one linear scan to find clause keywords at
/// depth-0 and insert `\n`, preserving every other character.
pub fn pretty_print(sql: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "SELECT",
        "FROM",
        "WHERE",
        "GROUP BY",
        "HAVING",
        "ORDER BY",
        "LIMIT",
        "OFFSET",
        "UNION ALL",
        "UNION",
        "INTERSECT ALL",
        "INTERSECT",
        "EXCEPT ALL",
        "EXCEPT",
    ];

    let chars: Vec<char> = sql.chars().collect();
    let mut out = String::with_capacity(sql.len() + 16);
    let mut paren_depth = 0i32;
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];

        // Track string literals and parens so we only break at
        // top-level clause boundaries.
        if !in_double_quote && c == '\'' {
            // toggle single-quote; respect SQL '' escape
            if in_single_quote && i + 1 < chars.len() && chars[i + 1] == '\'' {
                out.push(c);
                out.push(chars[i + 1]);
                i += 2;
                continue;
            }
            in_single_quote = !in_single_quote;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_single_quote && c == '"' {
            in_double_quote = !in_double_quote;
            out.push(c);
            i += 1;
            continue;
        }
        if !in_single_quote && !in_double_quote {
            if c == '(' {
                paren_depth += 1;
            } else if c == ')' {
                paren_depth = (paren_depth - 1).max(0);
            }
        }

        // At word boundary (space or start-of-string), probe for a
        // keyword we want to break before.
        if !in_single_quote && !in_double_quote && paren_depth == 0 && is_word_boundary(&chars, i) {
            if let Some((kw_text, kw_len)) = match_keyword(&chars, i, KEYWORDS) {
                // Only insert a break when we aren't already at
                // start-of-string and the previous emitted char isn't
                // a newline.
                if !out.is_empty() && !out.ends_with('\n') {
                    // Remove any trailing whitespace before injecting \n.
                    while out.ends_with(' ') {
                        out.pop();
                    }
                    out.push('\n');
                }
                out.push_str(&kw_text);
                i += kw_len;
                continue;
            }
        }

        out.push(c);
        i += 1;
    }

    out
}

/// True if `i` sits at a position where the preceding char is either
/// none (start of string) or whitespace — i.e. a keyword beginning
/// here wouldn't be mid-identifier.
fn is_word_boundary(chars: &[char], i: usize) -> bool {
    if i == 0 {
        return true;
    }
    let prev = chars[i - 1];
    prev.is_whitespace()
}

/// Case-insensitive match for the first keyword in `keywords` that
/// starts at `chars[i..]` AND is followed by a non-identifier char
/// (whitespace / punctuation / EOF). Returns the matched text (taken
/// from `chars`, preserving the original case) plus its length in
/// chars.
fn match_keyword(chars: &[char], i: usize, keywords: &[&str]) -> Option<(String, usize)> {
    for kw in keywords {
        let kw_chars: Vec<char> = kw.chars().collect();
        let n = kw_chars.len();
        if i + n > chars.len() {
            continue;
        }
        let slice = &chars[i..i + n];
        if slice
            .iter()
            .zip(kw_chars.iter())
            .all(|(a, b)| a.eq_ignore_ascii_case(b))
        {
            // Followed by non-identifier char (or end of string)?
            let at_boundary =
                i + n == chars.len() || (!chars[i + n].is_alphanumeric() && chars[i + n] != '_');
            if at_boundary {
                return Some((slice.iter().collect(), n));
            }
        }
    }
    None
}
