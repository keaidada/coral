// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Coverage tests for the full 30-rule function registry (Stage 1).
//!
//! These extend the SmokeDemo-based tests in `smoke_golden.rs` to hit the
//! function mappings that SmokeDemo itself doesn't exercise: POSITION,
//! BOOL_AND/OR, ARRAY_AGG, STRING_AGG, TRUNC, REGEXP_SUBSTR, GENERATE_SERIES,
//! TO_DATE / TO_TIMESTAMP / TO_CHAR.

use coral_core::{translate, unknown_functions};

fn assert_contains(input: &str, snippet: &str) {
    let got = translate(input).unwrap_or_else(|e| panic!("translate failed: {e}"));
    assert!(
        got.contains(snippet),
        "expected snippet `{snippet}` not in output.\ninput: {input}\noutput: {got}"
    );
}

fn assert_not_contains(input: &str, snippet: &str) {
    let got = translate(input).unwrap_or_else(|e| panic!("translate failed: {e}"));
    assert!(
        !got.contains(snippet),
        "unexpected snippet `{snippet}` found.\ninput: {input}\noutput: {got}"
    );
}

// ---------- aggregates / window passthrough (no change expected) ----------

#[test]
fn count_sum_avg_passthrough() {
    // These are Spark-compatible names in GaussDB; we should NOT rename them.
    assert_contains("SELECT COUNT(*) FROM t", "COUNT(*)");
    assert_contains("SELECT SUM(x), AVG(y), MIN(z), MAX(w) FROM t", "SUM(x)");
}

#[test]
fn coalesce_passthrough() {
    assert_contains("SELECT COALESCE(a, b, c) FROM t", "COALESCE(a, b, c)");
}

#[test]
fn row_number_rank_passthrough() {
    assert_contains(
        "SELECT ROW_NUMBER() OVER (ORDER BY x) FROM t",
        "ROW_NUMBER() OVER (ORDER BY x)",
    );
    assert_contains("SELECT RANK() OVER (ORDER BY x) FROM t", "RANK()");
    assert_contains(
        "SELECT DENSE_RANK() OVER (ORDER BY x) FROM t",
        "DENSE_RANK()",
    );
}

// ---------- renames ----------

#[test]
fn random_to_rand() {
    assert_contains("SELECT RANDOM() FROM t", "RAND()");
    // Ensure the original is gone — else we'd double-map on re-translate.
    assert_not_contains("SELECT RANDOM() FROM t", "RANDOM()");
}

#[test]
fn array_agg_to_collect_list() {
    assert_contains("SELECT ARRAY_AGG(x) FROM t", "COLLECT_LIST(x)");
}

#[test]
fn generate_series_to_sequence() {
    assert_contains("SELECT GENERATE_SERIES(1, 10) FROM t", "SEQUENCE(1, 10)");
    assert_contains(
        "SELECT GENERATE_SERIES(1, 10, 2) FROM t",
        "SEQUENCE(1, 10, 2)",
    );
}

#[test]
fn bool_and_bool_or() {
    assert_contains("SELECT BOOL_AND(x) FROM t", "EVERY(x)");
    assert_contains("SELECT BOOL_OR(x) FROM t", "SOME(x)");
}

// ---------- call-shape changes ----------

#[test]
fn position_swaps_and_renames() {
    // POSITION(needle, haystack) -> INSTR(haystack, needle)
    assert_contains("SELECT POSITION('a', name) FROM t", "INSTR(name, 'a')");
}

#[test]
fn string_agg_wraps_collect_list() {
    assert_contains(
        "SELECT STRING_AGG(name, ', ') FROM t",
        "CONCAT_WS(', ', COLLECT_LIST(name))",
    );
}

#[test]
fn trunc_date_swap() {
    // TRUNC(date_col, 'MM') -> DATE_TRUNC('MM', date_col)
    assert_contains(
        "SELECT TRUNC(created_at, 'MM') FROM t",
        "DATE_TRUNC('MM', created_at)",
    );
}

#[test]
fn trunc_numeric_passthrough() {
    // TRUNC with numeric second arg is semantically the same in both dialects
    // — no rewrite.
    assert_contains("SELECT TRUNC(x, 2) FROM t", "TRUNC(x, 2)");
}

#[test]
fn regexp_substr_to_extract_zero() {
    assert_contains(
        "SELECT REGEXP_SUBSTR(s, '\\d+') FROM t",
        "REGEXP_EXTRACT(s, '\\d+', 0)",
    );
}

#[test]
fn regexp_substr_drops_pos_occurrence() {
    // Optional pos/occurrence args are dropped (documented limitation).
    let got = translate("SELECT REGEXP_SUBSTR(s, 'pat', 1, 2) FROM t").unwrap();
    assert!(got.contains("REGEXP_EXTRACT(s, 'pat', 0)"), "got: {got}");
}

// ---------- date format token translation ----------

#[test]
fn to_char_date_to_date_format() {
    // to_char(d, 'YYYY-MM-DD') -> date_format(d, 'yyyy-MM-dd')
    assert_contains(
        "SELECT TO_CHAR(created_at, 'YYYY-MM-DD HH24:MI:SS') FROM t",
        "DATE_FORMAT(created_at, 'yyyy-MM-dd HH:mm:ss')",
    );
}

#[test]
fn to_char_numeric_passthrough() {
    // Numeric format (no date tokens) is NOT rewritten.
    assert_contains(
        "SELECT TO_CHAR(amount, 'fm999.99') FROM t",
        "TO_CHAR(amount, 'fm999.99')",
    );
}

#[test]
fn to_date_format_translated() {
    // to_date keeps its name; only the format literal is translated.
    assert_contains(
        "SELECT TO_DATE('2024-01-15', 'YYYY-MM-DD') FROM t",
        "TO_DATE('2024-01-15', 'yyyy-MM-dd')",
    );
}

#[test]
fn to_timestamp_format_translated() {
    assert_contains(
        "SELECT TO_TIMESTAMP('2024-01-15 10:30', 'YYYY-MM-DD HH24:MI') FROM t",
        "TO_TIMESTAMP('2024-01-15 10:30', 'yyyy-MM-dd HH:mm')",
    );
}

#[test]
fn to_timestamp_with_fractional_seconds() {
    assert_contains(
        "SELECT TO_TIMESTAMP(ts, 'YYYY-MM-DD HH24:MI:SS.FF6') FROM t",
        "TO_TIMESTAMP(ts, 'yyyy-MM-dd HH:mm:ss.SSSSSS')",
    );
}

// ---------- case sensitivity / robustness ----------

#[test]
fn lowercase_function_names_still_match() {
    assert_contains("SELECT nvl(x, 0) FROM t", "COALESCE(x, 0)");
    assert_contains("SELECT random() FROM t", "RAND()");
}

#[test]
fn mixed_case_function_names_still_match() {
    assert_contains("SELECT Nvl(x, 0) FROM t", "COALESCE(x, 0)");
    assert_contains("SELECT DeCoDe(x, 1, 'a') FROM t", "CASE WHEN");
}

#[test]
fn unknown_function_detection_flags_typos() {
    let unknown = unknown_functions("SELECT WNVL(x, 0), NVL(y, 0), WNVL(z, 1) FROM t").unwrap();
    assert_eq!(unknown, vec!["wnvl"]);
}

#[test]
fn unknown_function_detection_accepts_registered_functions() {
    let unknown = unknown_functions("SELECT NVL(x, 0), COALESCE(y, 0), RAND() FROM t").unwrap();
    assert!(unknown.is_empty(), "{unknown:?}");
}

#[test]
fn nested_stringagg_in_cte() {
    // Compound case — STRING_AGG nested inside a CTE.
    let input = "WITH agg AS (
        SELECT dept, STRING_AGG(name, ', ') AS names FROM emp GROUP BY dept
    ) SELECT * FROM agg";
    assert_contains(input, "CONCAT_WS(', ', COLLECT_LIST(name))");
}

#[test]
fn nvl2_then_decode_in_same_select() {
    // Multiple rewrites in a single SELECT — post-order visitor should handle.
    let input = "SELECT NVL2(x, 'a', 'b'), DECODE(y, 1, 'one', 'other') FROM t";
    let got = translate(input).unwrap();
    assert!(
        got.contains("CASE WHEN x IS NOT NULL THEN 'a' ELSE 'b' END"),
        "got: {got}"
    );
    assert!(
        got.contains("CASE WHEN y = 1 THEN 'one' ELSE 'other' END"),
        "got: {got}"
    );
}
