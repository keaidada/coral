// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Text-level pre-processor for Oracle / GaussDB `START WITH ... CONNECT BY`
//! (hierarchical query) syntax.
//!
//! `sqlparser-rs` 0.52 does not parse this syntax (it's Oracle-specific, and
//! GaussDB extended PG with it). Rather than forking the parser, we detect the
//! shape with a conservative regex-free scan and rewrite it into a
//! `WITH RECURSIVE` CTE that Spark can execute.
//!
//! ## Scope
//!
//! We handle the two common shapes from Coral's grammar-coverage doc:
//!
//! ```text
//! SELECT <cols>
//!   FROM <table>
//!   START WITH <start_predicate>
//!   CONNECT BY PRIOR <left> = <right>
//! ```
//! becomes
//! ```text
//! WITH RECURSIVE __coral_connect_by AS (
//!     SELECT * FROM <table> WHERE <start_predicate>
//!     UNION ALL
//!     SELECT <table>.* FROM <table>, __coral_connect_by
//!       WHERE __coral_connect_by.<left> = <right>
//! )
//! SELECT <cols> FROM __coral_connect_by
//! ```
//!
//! Anything more complex (multi-table FROM, nested CONNECT BY, PRIOR on the
//! right side, etc.) is left unmodified — the downstream parser will then
//! surface a syntax error with a precise position, which is better than us
//! producing silent nonsense.

const CTE_NAME: &str = "__coral_connect_by";

/// Rewrite every `START WITH ... CONNECT BY ...` block in `input` to
/// `WITH RECURSIVE`. Returns the input unchanged if no such block is found
/// or the shape is more complex than we can handle.
pub fn rewrite_connect_by(input: &str) -> String {
    // Fast path: no CONNECT BY anywhere.
    if !contains_keyword(input, "CONNECT BY") {
        return input.to_string();
    }

    match try_rewrite(input) {
        Some(s) => s,
        None => input.to_string(),
    }
}

fn try_rewrite(input: &str) -> Option<String> {
    // Find the anchor keywords (case-insensitive). We need:
    //   SELECT <cols> FROM <table> START WITH <pred> CONNECT BY PRIOR <l> = <r>
    let upper = input.to_uppercase();

    let select_pos = find_keyword(&upper, "SELECT")?;
    let from_pos = find_keyword_after(&upper, "FROM", select_pos)?;
    let start_with_pos = find_keyword_after(&upper, "START WITH", from_pos)?;
    let connect_by_pos = find_keyword_after(&upper, "CONNECT BY", start_with_pos)?;

    // Everything between FROM and START WITH is the table expression.
    let cols = input[select_pos + "SELECT".len()..from_pos].trim();
    let table_expr = input[from_pos + "FROM".len()..start_with_pos].trim();
    let start_pred =
        input[start_with_pos + "START WITH".len()..connect_by_pos].trim();

    // After CONNECT BY we expect `PRIOR <expr> = <expr>` optionally followed
    // by ORDER BY / GROUP BY etc. (which we don't currently propagate).
    let rest = input[connect_by_pos + "CONNECT BY".len()..].trim();
    let rest_upper = rest.to_uppercase();
    if !rest_upper.starts_with("PRIOR ") && !rest_upper.starts_with("PRIOR\t") {
        return None;
    }
    let connect_expr = rest["PRIOR".len()..].trim();

    // Find `<left> = <right>` — use the first top-level `=` we see.
    let eq_pos = find_top_level_eq(connect_expr)?;
    let left = connect_expr[..eq_pos].trim();
    let right = connect_expr[eq_pos + 1..].trim();
    // Strip any trailing clause (ORDER BY / LIMIT / ...) we don't support.
    let right = right
        .split_whitespace()
        .take_while(|t| {
            let u = t.to_uppercase();
            !matches!(u.as_str(), "ORDER" | "GROUP" | "LIMIT" | "OFFSET" | "HAVING")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let right = right.trim().trim_end_matches(';');

    // Bare-table-name check: we only rewrite when `table_expr` is a simple
    // identifier (no JOIN, no subquery, no alias). Otherwise the inline
    // `<table>.*` in the recursive branch isn't trivially correct.
    if !is_simple_identifier(table_expr) {
        return None;
    }

    let rewritten = format!(
        "WITH RECURSIVE {cte} AS (\n  \
           SELECT * FROM {table} WHERE {start_pred}\n  \
           UNION ALL\n  \
           SELECT {table}.* FROM {table}, {cte} WHERE {cte}.{left} = {right}\n\
         )\n\
         SELECT {cols} FROM {cte}",
        cte = CTE_NAME,
        table = table_expr,
        start_pred = start_pred,
        left = left,
        right = right,
        cols = cols,
    );

    Some(rewritten)
}

/// Case-insensitive substring check that respects word boundaries on the right
/// (left boundary unchecked — our anchors are distinctive enough).
fn contains_keyword(hay: &str, needle: &str) -> bool {
    find_keyword(&hay.to_uppercase(), needle).is_some()
}

fn find_keyword(upper: &str, needle: &str) -> Option<usize> {
    find_keyword_after(upper, needle, 0)
}

fn find_keyword_after(upper: &str, needle: &str, from: usize) -> Option<usize> {
    let mut start = from;
    while start < upper.len() {
        let pos = upper[start..].find(needle)? + start;
        let end = pos + needle.len();
        // Left boundary: preceding char must be whitespace / start-of-string
        // / non-ident.
        let left_ok = pos == 0
            || upper.as_bytes()[pos - 1].is_ascii_whitespace()
            || upper.as_bytes()[pos - 1] == b',';
        // Right boundary: trailing char must be whitespace / end-of-string.
        let right_ok = end == upper.len()
            || upper.as_bytes()[end].is_ascii_whitespace()
            || upper.as_bytes()[end] == b';';
        if left_ok && right_ok {
            return Some(pos);
        }
        start = end;
    }
    None
}

/// Find the first `=` that is at paren-depth 0. Returns byte index.
fn find_top_level_eq(s: &str) -> Option<usize> {
    let mut depth = 0usize;
    let mut in_string = false;
    let mut prev = 0u8;
    for (i, b) in s.bytes().enumerate() {
        match b {
            b'\'' if prev != b'\\' => in_string = !in_string,
            b'(' if !in_string => depth += 1,
            b')' if !in_string && depth > 0 => depth -= 1,
            b'=' if !in_string && depth == 0 => {
                // Skip `<=`, `>=`, `<>`, `!=`.
                if matches!(prev, b'<' | b'>' | b'!') {
                    prev = b;
                    continue;
                }
                return Some(i);
            }
            _ => {}
        }
        prev = b;
    }
    None
}

fn is_simple_identifier(s: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.')
        && !s.contains("..")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_connect_by_passthrough() {
        let sql = "SELECT id FROM t";
        assert_eq!(rewrite_connect_by(sql), sql);
    }

    #[test]
    fn rewrites_canonical_shape() {
        let sql = "SELECT id, name FROM employees
                   START WITH mgr_id IS NULL
                   CONNECT BY PRIOR id = mgr_id";
        let out = rewrite_connect_by(sql);
        assert!(out.contains("WITH RECURSIVE __coral_connect_by"));
        assert!(out.contains("UNION ALL"));
        assert!(out.contains("SELECT id, name FROM __coral_connect_by"));
    }

    #[test]
    fn leaves_complex_shapes_alone() {
        // JOIN in FROM — we don't rewrite these.
        let sql = "SELECT a FROM t1 JOIN t2 ON t1.k = t2.k
                   START WITH c = 1 CONNECT BY PRIOR id = pid";
        assert_eq!(rewrite_connect_by(sql), sql);
    }
}
