// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Golden tests — freeze the exact Spark SQL output for each of the 6 samples
//! lifted from `coral-gaussdb-spark/src/test/java/.../SmokeDemo.java`.
//!
//! When sqlparser-rs upgrades or we tweak a rewrite, these tests will flag the
//! diff so we can review intentional changes vs regressions.

use coral_core::translate;

fn assert_translates_to(input: &str, expected: &str) {
    let got = translate(input).unwrap_or_else(|e| panic!("translate failed: {e}"));
    // Normalize whitespace for comparison — sqlparser-rs's Display collapses
    // the input's formatting, so we compare after collapsing runs of spaces.
    let got_n = normalize(&got);
    let exp_n = normalize(expected);
    assert_eq!(
        got_n, exp_n,
        "\n--- INPUT ---\n{input}\n--- EXPECTED ---\n{expected}\n--- GOT ---\n{got}\n"
    );
}

fn normalize(s: &str) -> String {
    // Collapse all whitespace runs to single spaces and also normalize
    // paren-adjacent whitespace so `( x )` and `(x)` compare equal.
    let collapsed = s.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.replace("( ", "(").replace(" )", ")")
}

#[test]
fn sample_1_cte_join_window_nvl_concat() {
    let input = "WITH active_emp AS (
        SELECT id, name, dept_id, salary, mgr_id FROM employees WHERE salary > 0
     )
     SELECT
        d.name || ' / ' || NVL(e.name, 'n/a') AS label,
        COUNT(*) AS headcount,
        SUM(e.salary) AS total_pay,
        CASE WHEN AVG(e.salary) > 100 THEN 'high' ELSE 'low' END AS tier,
        ROW_NUMBER() OVER (PARTITION BY d.id ORDER BY SUM(e.salary) DESC) AS rn
     FROM active_emp e
     LEFT JOIN departments d ON e.dept_id = d.id
     GROUP BY d.id, d.name, e.name
     HAVING COUNT(*) > 0
     ORDER BY SUM(e.salary) DESC";
    // Key transformations we're asserting:
    //   NVL(e.name, 'n/a')  ->  COALESCE(e.name, 'n/a')
    // Everything else is dialect-neutral and round-trips unchanged.
    let expected = "WITH active_emp AS (SELECT id, name, dept_id, salary, mgr_id FROM employees WHERE salary > 0) \
        SELECT d.name || ' / ' || COALESCE(e.name, 'n/a') AS label, COUNT(*) AS headcount, \
        SUM(e.salary) AS total_pay, CASE WHEN AVG(e.salary) > 100 THEN 'high' ELSE 'low' END AS tier, \
        ROW_NUMBER() OVER (PARTITION BY d.id ORDER BY SUM(e.salary) DESC) AS rn \
        FROM active_emp AS e LEFT JOIN departments AS d ON e.dept_id = d.id \
        GROUP BY d.id, d.name, e.name HAVING COUNT(*) > 0 ORDER BY SUM(e.salary) DESC";
    assert_translates_to(input, expected);
}

#[test]
fn sample_2_pg_cast_decode_substr_mod() {
    let input = "SELECT
        id::BIGINT AS id64,
        DECODE(dept_id, 1, 'eng', 2, 'sales', 'other') AS dept_label,
        SUBSTR(name, 1, 3) AS short_name,
        MOD(id, 10) AS bucket
     FROM employees WHERE dept_id IN (1, 2, 3)";
    let expected = "SELECT CAST(id AS BIGINT) AS id64, \
        CASE WHEN dept_id = 1 THEN 'eng' WHEN dept_id = 2 THEN 'sales' ELSE 'other' END AS dept_label, \
        SUBSTRING(name, 1, 3) AS short_name, \
        id % 10 AS bucket \
        FROM employees WHERE dept_id IN (1, 2, 3)";
    assert_translates_to(input, expected);
}

#[test]
fn sample_3_regex_union_subquery() {
    let input = "SELECT id FROM employees WHERE name ~* '^a.*'
     UNION ALL
     SELECT id FROM employees WHERE dept_id IN (SELECT id FROM departments WHERE name ~ 'Eng')";
    // ~* -> case-insensitive RLIKE (wrap both sides in LOWER)
    // ~  -> RLIKE
    let expected = "SELECT id FROM employees WHERE LOWER(name) RLIKE LOWER('^a.*') \
        UNION ALL \
        SELECT id FROM employees WHERE dept_id IN (SELECT id FROM departments WHERE name RLIKE 'Eng')";
    assert_translates_to(input, expected);
}

#[test]
fn sample_4_merge_into_passthrough() {
    let input = "MERGE INTO employees t USING departments s ON t.dept_id = s.id
       WHEN MATCHED THEN UPDATE SET name = s.name
       WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)";
    // MERGE INTO is syntactically compatible between GaussDB and Spark (with
    // Delta / Iceberg table support). No rewrite needed.
    let expected = "MERGE INTO employees AS t USING departments AS s ON t.dept_id = s.id \
        WHEN MATCHED THEN UPDATE SET name = s.name \
        WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)";
    assert_translates_to(input, expected);
}

#[test]
fn sample_5_connect_by_to_with_recursive() {
    let input = "SELECT id, name FROM employees
       START WITH mgr_id IS NULL
       CONNECT BY PRIOR id = mgr_id";
    // Oracle hierarchical query -> standard SQL WITH RECURSIVE.
    let expected = "WITH RECURSIVE __coral_connect_by AS ( \
         SELECT * FROM employees WHERE mgr_id IS NULL \
         UNION ALL \
         SELECT employees.* FROM employees, __coral_connect_by \
           WHERE __coral_connect_by.id = mgr_id \
         ) SELECT id, name FROM __coral_connect_by";
    assert_translates_to(input, expected);
}

#[test]
fn sample_6_distinct_on_to_row_number() {
    let input = "SELECT DISTINCT ON (dept_id) id, dept_id, salary
       FROM employees ORDER BY dept_id, salary DESC";
    let expected = "SELECT id, dept_id, salary \
        FROM (SELECT id, dept_id, salary, \
                     ROW_NUMBER() OVER (PARTITION BY dept_id ORDER BY dept_id, salary DESC) \
                       AS __coral_distinct_on_rn \
              FROM employees) AS __coral_distinct_on_t \
        WHERE __coral_distinct_on_rn = 1";
    assert_translates_to(input, expected);
}

// ---------- Focused unit tests for individual rewrite rules ----------

#[test]
fn nvl2_expands_to_case() {
    let input = "SELECT NVL2(x, 'yes', 'no') FROM t";
    let got = translate(input).unwrap();
    assert!(
        got.contains("CASE WHEN x IS NOT NULL THEN 'yes' ELSE 'no' END"),
        "got: {got}"
    );
}

#[test]
fn pg_regex_not_match_inverts() {
    let input = "SELECT * FROM t WHERE x !~* 'foo'";
    let got = translate(input).unwrap();
    assert!(
        got.contains("NOT (LOWER(x) RLIKE LOWER('foo'))"),
        "got: {got}"
    );
}

#[test]
fn decode_without_default() {
    let input = "SELECT DECODE(x, 1, 'a', 2, 'b') FROM t";
    let got = translate(input).unwrap();
    assert!(
        got.contains("CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' END"),
        "got: {got}"
    );
    // No ELSE clause when the caller didn't give a default.
    assert!(!got.contains("ELSE"), "got: {got}");
}

#[test]
fn now_to_current_timestamp() {
    let input = "SELECT NOW() FROM t";
    let got = translate(input).unwrap();
    assert!(got.contains("CURRENT_TIMESTAMP"), "got: {got}");
}

#[test]
fn nested_nvl_rewrites_both_levels() {
    let input = "SELECT NVL(NVL(a, b), c) FROM t";
    let got = translate(input).unwrap();
    assert!(got.contains("COALESCE(COALESCE(a, b), c)"), "got: {got}");
}

#[test]
fn parse_error_surfaces() {
    let err = translate("SELEKT * FROM t").unwrap_err();
    assert!(err.to_string().contains("parse error"));
}
