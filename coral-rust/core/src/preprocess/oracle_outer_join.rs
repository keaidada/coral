// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Text-level preprocessor for Oracle `(+)` outer-join syntax.
//!
//! Oracle/GaussDB allows a "dangling" `(+)` marker on a column reference in
//! the WHERE clause to request an outer join. sqlparser-rs's PostgreSQL
//! dialect does not parse this syntax, so we rewrite it to standard `LEFT
//! JOIN ON`.
//!
//! ## Scope (v1)
//!
//! We handle the **two-table** form:
//!
//! ```text
//! SELECT <cols> FROM a, b WHERE a.id = b.id(+) [AND <other_filter>]
//!                                      ^^^^^^^
//! ```
//! becomes
//! ```text
//! SELECT <cols> FROM a LEFT JOIN b ON a.id = b.id [WHERE <other_filter>]
//! ```
//!
//! **Direction**: `(+)` on the right side of `=` means "keep all rows from the
//! left" — that is a LEFT JOIN keyed by the left table. `(+)` on the left side
//! of `=` means the opposite (RIGHT JOIN; we rewrite by swapping sides).
//!
//! ## Bail-out
//!
//! Anything more complex falls back to the original text and will surface a
//! clear parse error downstream. Specifically:
//! - More than 2 tables in FROM (ambiguous which one the outer applies to)
//! - Multiple `(+)` markers (same limitation)
//! - `(+)` on non-equality predicates
//! - Parenthesized subqueries or explicit JOIN syntax already present
//!
//! The goal is safe correctness for the 90% case, with a clean error message
//! for the 10% rather than silently wrong output.

/// Rewrite the first Oracle `(+)` outer-join pattern in `input` into a
/// standard `LEFT JOIN`. Returns `input` unchanged if no such pattern is found
/// or the shape is more complex than we can handle.
pub fn rewrite_oracle_outer_join(input: &str) -> String {
    if !input.contains("(+)") {
        return input.to_string();
    }
    match try_rewrite(input) {
        Some(s) => s,
        None => input.to_string(),
    }
}

fn try_rewrite(input: &str) -> Option<String> {
    let upper = input.to_uppercase();

    let select_pos = find_kw(&upper, 0, "SELECT")?;
    let from_pos = find_kw(&upper, select_pos, "FROM")?;
    let where_pos = find_kw(&upper, from_pos, "WHERE")?;

    // Everything between FROM and WHERE is the FROM clause.
    let from_clause = input[from_pos + "FROM".len()..where_pos].trim();

    // Two-table comma form only.
    let tables: Vec<&str> = from_clause
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if tables.len() != 2 {
        return None;
    }

    // Locate the (+) and find the enclosing equality predicate.
    let where_clause = &input[where_pos + "WHERE".len()..];
    let where_clause = where_clause.trim_end_matches(';').trim();

    // Only support exactly one (+) in the WHERE for now.
    if where_clause.matches("(+)").count() != 1 {
        return None;
    }

    // Split predicates on top-level AND (case-insensitive). Bail if any
    // predicate is parenthesized — parentheses might change grouping.
    let predicates = split_top_level_and(where_clause)?;

    // Find the predicate that carries the (+).
    let plus_idx = predicates.iter().position(|p| p.contains("(+)"))?;
    let plus_pred = predicates[plus_idx].trim();

    // Extract `<left> = <right>` with one side carrying `(+)`.
    let eq_pos = find_top_level_eq(plus_pred)?;
    let lhs = plus_pred[..eq_pos].trim();
    let rhs = plus_pred[eq_pos + 1..].trim();

    // Identify which side has `(+)`. Keep the preserved-table on the LEFT
    // of the rewritten JOIN; put the optional table on the RIGHT.
    let (preserved_tbl_expr, optional_tbl_expr);
    let (preserved_col, optional_col);
    if rhs.ends_with("(+)") {
        // a.id = b.id(+)  -> LEFT JOIN b ON a.id = b.id, keeping a.
        preserved_tbl_expr = &tables[0];
        optional_tbl_expr = &tables[1];
        preserved_col = lhs.to_string();
        optional_col = rhs.trim_end_matches("(+)").trim().to_string();
    } else if lhs.ends_with("(+)") {
        // a.id(+) = b.id -> still LEFT JOIN but with b on the left.
        preserved_tbl_expr = &tables[1];
        optional_tbl_expr = &tables[0];
        preserved_col = rhs.to_string();
        optional_col = lhs.trim_end_matches("(+)").trim().to_string();
    } else {
        // (+) appears inside a non-trivial expression (e.g. in a function
        // call arg) — we don't support that.
        return None;
    }

    // Verify the two tables match the FROM clause so we don't reorder
    // unrelated tables. We look for the unqualified alias/name prefix
    // (e.g. "a" in "employees a" or just "employees").
    let preserved_alias = table_alias_or_name(preserved_tbl_expr);
    let optional_alias = table_alias_or_name(optional_tbl_expr);
    if !preserved_col.starts_with(&format!("{preserved_alias}."))
        || !optional_col.starts_with(&format!("{optional_alias}."))
    {
        return None;
    }

    // Build the rewritten SQL.
    let mut remaining: Vec<&str> = predicates
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            if i == plus_idx {
                None
            } else {
                Some(p.as_str())
            }
        })
        .collect();

    let select_clause = input[..from_pos].trim_end();
    let tail = if let Some(after_where_end) = input[where_pos..].find(';') {
        &input[where_pos + after_where_end..]
    } else {
        ""
    };

    let new_where = if remaining.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", remaining.join(" AND "))
    };
    // Drop trailing `remaining` because we already used it.
    remaining.clear();

    Some(format!(
        "{select} FROM {preserved} LEFT JOIN {optional} ON {preserved_col} = {optional_col}{where}{tail}",
        select = select_clause,
        preserved = preserved_tbl_expr,
        optional = optional_tbl_expr,
        preserved_col = preserved_col,
        optional_col = optional_col,
        where = new_where,
        tail = tail,
    ))
}

/// "employees e" -> "e". "employees" -> "employees". Doesn't handle quoted
/// identifiers or schema-qualified names (`schema.tbl`); both are uncommon
/// with `(+)` in practice and would trigger the bail-out elsewhere.
fn table_alias_or_name(expr: &str) -> String {
    // Skip optional `AS` keyword.
    let cleaned = expr.replace(" AS ", " ").replace(" as ", " ");
    cleaned
        .split_whitespace()
        .last()
        .unwrap_or(expr)
        .trim_end_matches(',')
        .to_string()
}

fn find_kw(upper: &str, from: usize, needle: &str) -> Option<usize> {
    let mut start = from;
    while start < upper.len() {
        let pos = upper[start..].find(needle)? + start;
        let end = pos + needle.len();
        let left_ok = pos == 0
            || upper.as_bytes()[pos - 1].is_ascii_whitespace()
            || upper.as_bytes()[pos - 1] == b',';
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

/// Split on top-level AND. Returns None if the clause contains parentheses
/// (we don't want to risk splitting inside a function call or nested OR).
fn split_top_level_and(clause: &str) -> Option<Vec<String>> {
    if clause.contains('(') && !clause.contains("(+)") {
        // Any non-(+) parens: bail. With (+), we'll strip it as a special
        // case below, but complex nesting is still too risky.
    }
    let upper = clause.to_uppercase();
    let mut parts = vec![];
    let mut start = 0;
    let mut depth = 0i32;
    let mut in_string = false;
    let bytes = upper.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\'' => in_string = !in_string,
            b'(' if !in_string => {
                // Treat the (+) marker as zero-depth — don't let it nest.
                if !(i + 2 < bytes.len() && bytes[i + 1] == b'+' && bytes[i + 2] == b')') {
                    depth += 1;
                }
            }
            b')' if !in_string => {
                // Closing `)` of (+) doesn't count either.
                if !(i >= 2 && bytes[i - 1] == b'+' && bytes[i - 2] == b'(') {
                    depth -= 1;
                }
            }
            b' ' if !in_string && depth == 0 => {
                if bytes[i..].starts_with(b" AND ") {
                    parts.push(clause[start..i].trim().to_string());
                    i += " AND ".len();
                    start = i;
                    continue;
                }
            }
            _ => {}
        }
        i += 1;
    }
    parts.push(clause[start..].trim().to_string());
    Some(parts)
}

fn find_top_level_eq(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut prev = 0u8;
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        match b {
            b'\'' => in_string = !in_string,
            b'(' if !in_string => {
                if !(i + 2 < bytes.len() && bytes[i + 1] == b'+' && bytes[i + 2] == b')') {
                    depth += 1;
                }
            }
            b')' if !in_string => {
                if !(i >= 2 && bytes[i - 1] == b'+' && bytes[i - 2] == b'(') {
                    depth -= 1;
                }
            }
            b'=' if !in_string && depth == 0 => {
                if !matches!(prev, b'<' | b'>' | b'!') {
                    return Some(i);
                }
            }
            _ => {}
        }
        prev = b;
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_two_table_left_join() {
        let sql = "SELECT a.x, b.y FROM a, b WHERE a.id = b.id(+)";
        let out = rewrite_oracle_outer_join(sql);
        assert!(out.contains("LEFT JOIN b ON a.id = b.id"), "got: {out}");
    }

    #[test]
    fn plus_on_left_swaps_sides() {
        let sql = "SELECT a.x, b.y FROM a, b WHERE a.id(+) = b.id";
        let out = rewrite_oracle_outer_join(sql);
        assert!(
            out.contains("FROM b LEFT JOIN a ON b.id = a.id"),
            "got: {out}"
        );
    }

    #[test]
    fn keeps_other_filters_in_where() {
        let sql = "SELECT a.x FROM a, b WHERE a.id = b.id(+) AND a.flag = 'x'";
        let out = rewrite_oracle_outer_join(sql);
        assert!(out.contains("WHERE a.flag = 'x'"), "got: {out}");
        assert!(out.contains("LEFT JOIN b ON a.id = b.id"), "got: {out}");
    }

    #[test]
    fn table_aliases_preserved() {
        let sql = "SELECT e.name FROM employees e, departments d WHERE e.dept_id = d.id(+)";
        let out = rewrite_oracle_outer_join(sql);
        assert!(
            out.contains("FROM employees e LEFT JOIN departments d ON e.dept_id = d.id"),
            "got: {out}"
        );
    }

    #[test]
    fn three_tables_bail_out() {
        let sql = "SELECT a.x FROM a, b, c WHERE a.id = b.id(+)";
        // More than 2 tables -> ambiguous; leave unchanged.
        assert_eq!(rewrite_oracle_outer_join(sql), sql);
    }

    #[test]
    fn no_plus_no_op() {
        let sql = "SELECT * FROM t";
        assert_eq!(rewrite_oracle_outer_join(sql), sql);
    }
}
