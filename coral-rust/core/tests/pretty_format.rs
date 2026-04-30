// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Tests for the Calcite-style pretty-printer (`translate_to_with(..,
//! pretty=true)` / the HTTP service's output). These pin the output
//! byte-for-byte to what Java Coral emits, so the frontend gets the
//! same rendered SQL regardless of whether the backend is Java or
//! Rust.

use coral_core::{translate_to_with, Target};

fn normalize_lines(s: &str) -> Vec<&str> {
    s.lines().map(str::trim).filter(|l| !l.is_empty()).collect()
}

// ---------- Spark target (matches Java translateHiveToSpark output) ----

#[test]
fn plain_select_spark_matches_java() {
    // Java Calcite output:
    //   SELECT *\nFROM default.user user          (or SELECT id, name if projection survives)
    //
    // Our coral-core keeps the explicit projection (no `SELECT *`
    // rewriting) but matches every other piece: default-qualified
    // table, no AS between table and alias, clauses on separate
    // lines.
    let out = translate_to_with("SELECT id, name FROM user", Target::Spark, true).unwrap();
    let lines = normalize_lines(&out);
    assert_eq!(
        lines,
        vec!["SELECT id, name", "FROM default.user user"],
        "{out}"
    );
}

#[test]
fn spark_drops_as_keyword_in_from() {
    let out = translate_to_with("SELECT id FROM t", Target::Spark, true).unwrap();
    assert!(out.contains("FROM default.t t"), "{out}");
    assert!(
        !out.contains("AS t"),
        "expected no AS between table and alias: {out}"
    );
}

// ---------- Trino target (matches Java translateHiveToTrino output) ----

#[test]
fn plain_select_trino_matches_java() {
    // Java Trino output:
    //   SELECT *\nFROM "default"."user" AS "user"
    //
    // Double-quoted identifiers, AS kept, db-qualified.
    let out = translate_to_with("SELECT id, name FROM user", Target::Trino, true).unwrap();
    let lines = normalize_lines(&out);
    assert_eq!(
        lines,
        vec!["SELECT id, name", r#"FROM "default"."user" AS "user""#],
        "{out}"
    );
}

#[test]
fn trino_keeps_as_and_quotes_identifiers() {
    let out = translate_to_with("SELECT id FROM t", Target::Trino, true).unwrap();
    assert!(out.contains(r#"FROM "default"."t" AS "t""#), "{out}");
}

// ---------- Layout: multi-clause ----------

#[test]
fn where_group_having_order_each_on_own_line() {
    let sql = "SELECT dept, COUNT(*) n FROM emp WHERE salary > 0 GROUP BY dept HAVING COUNT(*) > 3 ORDER BY n DESC LIMIT 10";
    let out = translate_to_with(sql, Target::Spark, true).unwrap();
    let lines = normalize_lines(&out);
    assert!(lines.iter().any(|l| l.starts_with("SELECT")), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("FROM")), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("WHERE")), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("GROUP BY")), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("HAVING")), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("ORDER BY")), "{out}");
    assert!(lines.iter().any(|l| l.starts_with("LIMIT")), "{out}");
}

// ---------- db-qualified names stay untouched ----------

#[test]
fn qualified_table_not_re_prefixed() {
    // `hr.employees` is already qualified; we MUST NOT prepend
    // `default.` to it.
    let out = translate_to_with("SELECT id FROM hr.employees", Target::Spark, true).unwrap();
    assert!(out.contains("FROM hr.employees employees"), "{out}");
    assert!(!out.contains("default.hr"), "{out}");
}

// ---------- existing user-supplied alias is preserved ----------

#[test]
fn existing_alias_is_preserved() {
    let out = translate_to_with("SELECT e.id FROM employees e", Target::Spark, true).unwrap();
    assert!(out.contains("FROM default.employees e"), "{out}");
    // Must NOT replace the user's "e" alias with "employees".
    assert!(!out.contains("employees employees"), "{out}");
}

// ---------- UNION ----------

#[test]
fn union_all_keyword_starts_new_line() {
    let out = translate_to_with(
        "SELECT id FROM a UNION ALL SELECT id FROM b",
        Target::Spark,
        true,
    )
    .unwrap();
    let lines = normalize_lines(&out);
    assert!(lines.iter().any(|l| l.starts_with("UNION ALL")), "{out}");
}

// ---------- compact mode unchanged ----------

#[test]
fn compact_mode_still_works_for_existing_callers() {
    // translate_to() uses pretty=false — the golden tests depend on it.
    let compact = coral_core::translate_to("SELECT id FROM user", Target::Spark).unwrap();
    assert_eq!(compact, "SELECT id FROM user");
    // Explicit pretty=false matches translate_to().
    let also_compact = translate_to_with("SELECT id FROM user", Target::Spark, false).unwrap();
    assert_eq!(also_compact, compact);
}

// ---------- paren-depth tracking ----------

#[test]
fn parens_suppress_inner_clause_breaks() {
    let out = translate_to_with(
        "SELECT id FROM (SELECT id FROM t WHERE x > 0) sub",
        Target::Spark,
        true,
    )
    .unwrap();
    let lines: Vec<&str> = out.lines().filter(|l| !l.trim().is_empty()).collect();
    assert_eq!(lines.len(), 2, "{out}");
    assert!(lines[0].trim_start().starts_with("SELECT"), "{out}");
    assert!(lines[1].trim_start().starts_with("FROM"), "{out}");
    // Inner subquery's FROM / WHERE must not trigger extra breaks.
    assert!(lines[1].contains("WHERE x > 0)"), "{out}");
}

// ---------- string literal safety ----------

#[test]
fn string_literals_with_clause_keywords_do_not_break() {
    let out = translate_to_with(
        "SELECT 'SELECT * FROM test' AS msg FROM t",
        Target::Spark,
        true,
    )
    .unwrap();
    assert!(out.contains("'SELECT * FROM test'"), "{out}");
    let lines = normalize_lines(&out);
    assert!(
        lines.iter().any(|l| l.starts_with("FROM default.t")),
        "{out}"
    );
}
