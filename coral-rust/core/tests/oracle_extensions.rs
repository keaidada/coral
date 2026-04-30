// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Stage-3 tests: Oracle `(+)` outer joins, window frames, and assorted
//! GaussDB extensions that go beyond the basic SmokeDemo scenarios.

use coral_core::translate;

fn assert_contains(input: &str, snippet: &str) {
    let got = translate(input).unwrap_or_else(|e| panic!("translate failed: {e}"));
    assert!(
        got.contains(snippet),
        "expected `{snippet}` in output.\ninput: {input}\noutput: {got}"
    );
}

// ---------- Oracle `(+)` outer joins ----------

#[test]
fn oracle_plus_rewrites_to_left_join() {
    let input = "SELECT a.x, b.y FROM a, b WHERE a.id = b.id(+)";
    let got = translate(input).unwrap();
    assert!(
        got.contains("LEFT JOIN b ON a.id = b.id"),
        "got: {got}"
    );
    assert!(!got.contains("(+)"), "got: {got}");
}

#[test]
fn oracle_plus_with_aliases() {
    let input =
        "SELECT e.name, d.name FROM employees e, departments d WHERE e.dept_id = d.id(+)";
    let got = translate(input).unwrap();
    assert!(
        got.contains("FROM employees AS e LEFT JOIN departments AS d ON e.dept_id = d.id"),
        "got: {got}"
    );
}

#[test]
fn oracle_plus_with_extra_filter() {
    let input =
        "SELECT a.x FROM a, b WHERE a.id = b.id(+) AND a.flag = 'active'";
    let got = translate(input).unwrap();
    assert!(got.contains("LEFT JOIN b ON a.id = b.id"), "got: {got}");
    assert!(got.contains("WHERE a.flag = 'active'"), "got: {got}");
}

#[test]
fn oracle_plus_left_side_swaps() {
    // a.id(+) = b.id  means "keep rows from b". We rewrite to put b on the
    // left of LEFT JOIN.
    let input = "SELECT a.x, b.y FROM a, b WHERE a.id(+) = b.id";
    let got = translate(input).unwrap();
    assert!(
        got.contains("FROM b LEFT JOIN a ON b.id = a.id"),
        "got: {got}"
    );
}

// ---------- window frames (sqlparser-rs handles these natively) ----------

#[test]
fn rows_between_preceding_frame() {
    let input = "SELECT id, SUM(x) OVER (ORDER BY id ROWS BETWEEN 3 PRECEDING AND CURRENT ROW) AS rolling FROM t";
    assert_contains(
        input,
        "SUM(x) OVER (ORDER BY id ROWS BETWEEN 3 PRECEDING AND CURRENT ROW)",
    );
}

#[test]
fn range_between_unbounded_frame() {
    let input = "SELECT SUM(x) OVER (ORDER BY ts RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM t";
    assert_contains(input, "RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW");
}

#[test]
fn rows_between_n_preceding_and_following() {
    let input = "SELECT AVG(x) OVER (PARTITION BY k ORDER BY id ROWS BETWEEN 5 PRECEDING AND 5 FOLLOWING) FROM t";
    assert_contains(input, "ROWS BETWEEN 5 PRECEDING AND 5 FOLLOWING");
}

// ---------- CONNECT BY / recursive variants ----------

#[test]
fn connect_by_to_with_recursive() {
    let input = "SELECT id, name FROM employees
       START WITH mgr_id IS NULL
       CONNECT BY PRIOR id = mgr_id";
    assert_contains(input, "WITH RECURSIVE __coral_connect_by");
    assert_contains(input, "UNION ALL");
}

#[test]
fn connect_by_with_complex_start_predicate() {
    // The preprocessor captures everything between START WITH and CONNECT BY
    // as the starting predicate.
    let input = "SELECT id FROM employees
       START WITH mgr_id IS NULL AND active = true
       CONNECT BY PRIOR id = mgr_id";
    let got = translate(input).unwrap();
    assert!(
        got.contains("mgr_id IS NULL AND active = true"),
        "got: {got}"
    );
}

// ---------- interaction with other rewrites ----------

#[test]
fn oracle_plus_plus_function_rewrite() {
    // (+) should be cleaned up, then the function inside is rewritten.
    let input = "SELECT NVL(b.x, 'n/a') FROM a, b WHERE a.id = b.id(+)";
    let got = translate(input).unwrap();
    assert!(got.contains("LEFT JOIN b ON a.id = b.id"), "got: {got}");
    assert!(got.contains("COALESCE(b.x, 'n/a')"), "got: {got}");
}

#[test]
fn connect_by_plus_function_rewrite() {
    // CONNECT BY rewrite happens at text level; inner functions still get
    // AST-level rewrites after sqlparser parses the WITH RECURSIVE output.
    let input = "SELECT NVL(name, 'unknown') FROM employees
       START WITH mgr_id IS NULL
       CONNECT BY PRIOR id = mgr_id";
    let got = translate(input).unwrap();
    assert!(got.contains("WITH RECURSIVE"), "got: {got}");
    assert!(got.contains("COALESCE(name, 'unknown')"), "got: {got}");
}
