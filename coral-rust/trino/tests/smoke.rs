// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// End-to-end tests for the Trino output backend. Each test passes a chunk
// of GaussDB / Hive SQL through `to_trino_sql` and asserts the output
// contains the Trino-specific rewrite (and does NOT contain the Spark-only
// form) — that's what proves the Trino pass actually ran on top of the
// Spark pass.

use coral_trino::{to_trino_sql, to_trino_sql_all};

fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn contains_ci(haystack: &str, needle: &str) -> bool {
    haystack.to_ascii_uppercase().contains(&needle.to_ascii_uppercase())
}

#[test]
fn nvl_becomes_coalesce_via_spark_pass() {
    // NVL lives in the Spark pass, not the Trino pass — but Trino wants
    // COALESCE too, so this should pass through cleanly.
    let got = to_trino_sql("SELECT NVL(name, 'x') FROM t").unwrap();
    assert!(contains_ci(&got, "COALESCE"), "{got}");
    assert!(!contains_ci(&got, "NVL("), "{got}");
}

#[test]
fn rand_becomes_random_no_args() {
    let got = to_trino_sql("SELECT RAND() FROM t").unwrap();
    assert!(contains_ci(&got, "RANDOM("), "{got}");
    assert!(!contains_ci(&got, "RAND("), "{got}");
}

#[test]
fn rand_with_seed_becomes_random_no_args() {
    // RAND(42) in Hive is not a Trino concept; we drop the seed.
    let got = to_trino_sql("SELECT RAND(42) FROM t").unwrap();
    assert!(contains_ci(&got, "RANDOM()"), "{got}");
}

#[test]
fn get_json_object_becomes_json_extract() {
    let got = to_trino_sql("SELECT GET_JSON_OBJECT(p, '$.k') FROM t").unwrap();
    assert!(contains_ci(&got, "JSON_EXTRACT"), "{got}");
    assert!(!contains_ci(&got, "GET_JSON_OBJECT"), "{got}");
}

#[test]
fn array_contains_becomes_contains() {
    let got = to_trino_sql("SELECT ARRAY_CONTAINS(tags, 'x') FROM t").unwrap();
    assert!(contains_ci(&got, "CONTAINS(tags"), "{got}");
    assert!(!contains_ci(&got, "ARRAY_CONTAINS"), "{got}");
}

#[test]
fn base64_family_rewrites() {
    let got = to_trino_sql(
        "SELECT BASE64(x), UNBASE64(y), HEX(z), UNHEX(w) FROM t",
    )
    .unwrap();
    assert!(contains_ci(&got, "TO_BASE64(x)"), "{got}");
    assert!(contains_ci(&got, "FROM_BASE64(y)"), "{got}");
    assert!(contains_ci(&got, "TO_HEX(z)"), "{got}");
    assert!(contains_ci(&got, "FROM_HEX(w)"), "{got}");
}

#[test]
fn instr_becomes_strpos() {
    let got = to_trino_sql("SELECT INSTR(s, 'x') FROM t").unwrap();
    assert!(contains_ci(&got, "STRPOS"), "{got}");
    assert!(!contains_ci(&got, "INSTR("), "{got}");
}

#[test]
fn collect_list_becomes_array_agg() {
    let got = to_trino_sql("SELECT COLLECT_LIST(x) FROM t GROUP BY k").unwrap();
    assert!(contains_ci(&got, "ARRAY_AGG(x)"), "{got}");
    assert!(!contains_ci(&got, "COLLECT_LIST"), "{got}");
}

#[test]
fn collect_set_becomes_array_agg_distinct() {
    let got = to_trino_sql("SELECT COLLECT_SET(x) FROM t GROUP BY k").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("ARRAY_AGG(DISTINCT X)"), "{n}");
}

#[test]
fn pmod_expands_to_conditional_modulo() {
    let got = to_trino_sql("SELECT PMOD(a, b) FROM t").unwrap();
    // Trino doesn't have PMOD — expected form: ((a % b) + b) % b
    assert!(contains_ci(&got, "% b"), "{got}");
    assert!(!contains_ci(&got, "PMOD"), "{got}");
}

#[test]
fn date_add_adds_unit_literal() {
    let got = to_trino_sql("SELECT DATE_ADD(d, 30) FROM t").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("'DAY'") || n.contains("'day'".to_uppercase().as_str()), "{n}");
    assert!(n.contains("CAST(D AS DATE)"), "{n}");
}

#[test]
fn date_sub_becomes_negated_date_add() {
    let got = to_trino_sql("SELECT DATE_SUB(d, 7) FROM t").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("DATE_ADD"), "{n}");
    assert!(n.contains("- 7") || n.contains("-7"), "{n}");
    assert!(!n.contains("DATE_SUB"), "{n}");
}

#[test]
fn datediff_becomes_date_diff_reversed() {
    let got = to_trino_sql("SELECT DATEDIFF(a, b) FROM t").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("DATE_DIFF"), "{n}");
    assert!(n.contains("CAST(B AS DATE), CAST(A AS DATE)"), "{n}");
}

#[test]
fn to_date_single_arg_becomes_date_cast() {
    let got = to_trino_sql("SELECT TO_DATE(s) FROM t").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("DATE(CAST(S AS TIMESTAMP"), "{n}");
}

#[test]
fn to_date_two_arg_left_alone() {
    // We don't risk silently reshaping user-supplied formats.
    let got = to_trino_sql("SELECT TO_DATE(s, 'yyyy-MM-dd') FROM t").unwrap();
    assert!(contains_ci(&got, "TO_DATE"), "{got}");
}

#[test]
fn pg_regex_operator_lands_as_regexp_like_in_trino() {
    // GaussDB uses `~` for regex match. The Spark pass rewrites it to the
    // RLIKE binary operator; the Trino pass then lifts that into the
    // REGEXP_LIKE function (Trino's regex-match form).
    let got = to_trino_sql("SELECT * FROM t WHERE name ~ '^foo'").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("REGEXP_LIKE"), "{n}");
    assert!(!n.contains(" RLIKE "), "{n}");
    assert!(!n.contains(" ~ '"), "{n}");
}

#[test]
fn spark_specific_oracle_plus_still_works() {
    // (+) comes from the Spark preprocessor and must survive the Trino
    // path (it's a text-level preprocessor, not dialect-specific).
    let got = to_trino_sql(
        "SELECT * FROM a, b WHERE a.id = b.id(+)",
    )
    .unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("LEFT JOIN") || n.contains("LEFT OUTER JOIN"), "{n}");
    assert!(!n.contains("(+)"), "{n}");
}

#[test]
fn distinct_on_rewrite_also_works_for_trino() {
    // DISTINCT ON is a Spark-path rewrite (ROW_NUMBER subquery). Just
    // confirms the Trino pass didn't break it.
    let got = to_trino_sql("SELECT DISTINCT ON (k) k, v FROM t").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("ROW_NUMBER"), "{n}");
}

#[test]
fn gaussdb_jsonb_cast_survives_to_trino() {
    // ::JSONB is rewritten to CAST(x AS STRING) by the Spark types pass
    // and then STRING → VARCHAR by the Trino types pass.
    let got = to_trino_sql("SELECT x::JSONB FROM t").unwrap();
    let n = normalize(&got).to_ascii_uppercase();
    assert!(n.contains("CAST(X AS "), "{n}");
    // Should be VARCHAR now, not STRING.
    assert!(n.contains("VARCHAR"), "{n}");
    assert!(!n.contains("::JSONB"), "{n}");
}

#[test]
fn multi_statement_to_trino_sql_all() {
    let got = to_trino_sql_all("SELECT RAND(); SELECT NVL(x,y) FROM t").unwrap();
    assert_eq!(got.len(), 2);
    assert!(contains_ci(&got[0], "RANDOM"), "{}", got[0]);
    assert!(contains_ci(&got[1], "COALESCE"), "{}", got[1]);
}

#[test]
fn unknown_function_passes_through_untouched() {
    // A function with no Spark and no Trino rewrite must reach Trino
    // verbatim; Trino itself will decide whether to accept it.
    let got = to_trino_sql("SELECT MY_CUSTOM_UDF(x) FROM t").unwrap();
    assert!(contains_ci(&got, "MY_CUSTOM_UDF"), "{got}");
}

#[test]
fn trino_output_is_idempotent_modulo_whitespace() {
    // Running translation twice should produce the same SQL after
    // whitespace normalization — the Trino pass must not introduce
    // GaussDB-isms that would re-fire Spark-pass rewrites.
    let inputs = [
        "SELECT NVL(a, b), RAND(), DATE_ADD(d, 1) FROM t",
        "SELECT COLLECT_LIST(x), PMOD(a, b) FROM t GROUP BY k",
        "SELECT * FROM t WHERE s RLIKE '^x'",
    ];
    for sql in inputs {
        let first = to_trino_sql(sql).unwrap();
        let second = to_trino_sql(&first).unwrap();
        assert_eq!(normalize(&first), normalize(&second), "in={sql}");
    }
}
