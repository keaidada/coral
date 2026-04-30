// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! End-to-end tests for Stage-2 type mapping: GaussDB types inside CAST
//! expressions and CREATE TABLE columns get translated to Spark types.

use coral_core::translate;

fn assert_contains(input: &str, snippet: &str) {
    let got = translate(input).unwrap_or_else(|e| panic!("translate failed: {e}"));
    assert!(
        got.contains(snippet),
        "expected `{snippet}` in output.\ninput: {input}\noutput: {got}"
    );
}

// ---------- casts ----------

#[test]
fn cast_json_to_string() {
    assert_contains("SELECT x::JSON FROM t", "CAST(x AS STRING)");
}

#[test]
fn cast_jsonb_to_string() {
    assert_contains("SELECT x::JSONB FROM t", "CAST(x AS STRING)");
}

#[test]
fn cast_uuid_to_string() {
    assert_contains("SELECT x::UUID FROM t", "CAST(x AS STRING)");
}

#[test]
fn cast_bytea_to_binary() {
    assert_contains("SELECT x::BYTEA FROM t", "CAST(x AS BINARY)");
}

#[test]
fn cast_text_to_string() {
    assert_contains("SELECT x::TEXT FROM t", "CAST(x AS STRING)");
}

#[test]
fn cast_timestamptz_strips_tz() {
    assert_contains("SELECT x::TIMESTAMPTZ FROM t", "CAST(x AS TIMESTAMP)");
}

#[test]
fn cast_int4_to_int() {
    assert_contains("SELECT x::INT4 FROM t", "CAST(x AS INT)");
}

#[test]
fn cast_int8_to_bigint() {
    assert_contains("SELECT x::INT8 FROM t", "CAST(x AS BIGINT)");
}

#[test]
fn cast_float8_to_double() {
    assert_contains("SELECT x::FLOAT8 FROM t", "CAST(x AS DOUBLE)");
}

#[test]
fn cast_float4_to_real() {
    assert_contains("SELECT x::FLOAT4 FROM t", "CAST(x AS REAL)");
}

// ---------- CREATE TABLE ----------

#[test]
fn create_table_json_column_to_string() {
    assert_contains("CREATE TABLE events (id BIGINT, payload JSONB)", "STRING");
}

#[test]
fn create_table_uuid_and_bytea() {
    let got = translate("CREATE TABLE files (id UUID, checksum BYTEA, body TEXT)").unwrap();
    assert!(got.contains("STRING"), "got: {got}"); // from UUID and TEXT
    assert!(got.contains("BINARY"), "got: {got}"); // from BYTEA
}

#[test]
fn create_table_timestamptz_column() {
    // TIMESTAMPTZ column -> TIMESTAMP
    let got = translate("CREATE TABLE events (id BIGINT, created_at TIMESTAMPTZ)").unwrap();
    assert!(
        got.contains("TIMESTAMP")
            && !got.contains("TIMESTAMPTZ")
            && !got.contains("WITH TIME ZONE"),
        "got: {got}"
    );
}

#[test]
fn create_table_serial_columns() {
    // SERIAL / BIGSERIAL are Custom types — confirm the fold to INT / BIGINT.
    let got = translate("CREATE TABLE things (id SERIAL, big_id BIGSERIAL, small_id SMALLSERIAL)")
        .unwrap();
    assert!(got.contains("INT"), "got: {got}");
    assert!(got.contains("BIGINT"), "got: {got}");
    assert!(got.contains("SMALLINT"), "got: {got}");
    // SERIAL shouldn't leak through.
    assert!(!got.to_uppercase().contains("SERIAL"), "got: {got}");
}

// ---------- untouched types ----------

#[test]
fn common_types_passthrough() {
    // These pass through untouched (Spark-compatible spelling).
    assert_contains("SELECT x::INT FROM t", "CAST(x AS INT)");
    assert_contains("SELECT x::BIGINT FROM t", "CAST(x AS BIGINT)");
    assert_contains("SELECT x::VARCHAR(10) FROM t", "CAST(x AS VARCHAR(10))");
    assert_contains(
        "SELECT x::DECIMAL(18, 2) FROM t",
        "CAST(x AS DECIMAL(18,2))",
    );
    assert_contains("SELECT x::DOUBLE FROM t", "CAST(x AS DOUBLE)");
    assert_contains("SELECT x::BOOLEAN FROM t", "CAST(x AS BOOLEAN)");
    assert_contains("SELECT x::DATE FROM t", "CAST(x AS DATE)");
}

#[test]
fn interval_passthrough() {
    // Spark 3+ has INTERVAL, so we preserve it.
    assert_contains("SELECT x::INTERVAL FROM t", "CAST(x AS INTERVAL)");
}

#[test]
fn plain_timestamp_unchanged() {
    // Plain TIMESTAMP (no timezone) must NOT be touched.
    assert_contains("SELECT x::TIMESTAMP FROM t", "CAST(x AS TIMESTAMP)");
}

// ---------- interaction with function rewriter ----------

#[test]
fn cast_inside_rewritten_function() {
    // The type rewriter must run on casts introduced by earlier passes too.
    // `NVL(x::JSONB, '{}')` -> `COALESCE(CAST(x AS STRING), '{}')`.
    let got = translate("SELECT NVL(x::JSONB, '{}') FROM t").unwrap();
    assert!(
        got.contains("COALESCE(CAST(x AS STRING), '{}')"),
        "got: {got}"
    );
}
