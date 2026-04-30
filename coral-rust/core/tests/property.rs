// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Property-based smoke tests.
//!
//! These run on stable Rust (no nightly / no libFuzzer). They don't
//! replace real fuzzing (that's cargo-fuzz, see coral-rust/fuzz/) but they
//! add a cheap regression layer for the basic invariants:
//!
//! 1. `translate()` never panics on any string (parse errors are fine).
//! 2. For SQL that parses successfully, the output must NOT contain the
//!    GaussDB-only tokens we promised to rewrite:
//!       - `(+)` (Oracle outer join)
//!       - `::` style casts
//!       - space-bounded PG regex operators
//! 3. `translate(translate(x))` is idempotent modulo whitespace.
//!
//! The "generator" is deliberately shallow: it shuffles fragments from a
//! small dictionary of keywords + identifiers + operators into strings. It's
//! not trying to generate valid SQL — it's trying to feed the parser a wide
//! variety of inputs to catch parse paths that happen to reach the rewriter.

use coral_core::translate;

const FRAGMENTS: &[&str] = &[
    "SELECT",
    "FROM",
    "WHERE",
    "AND",
    "OR",
    "GROUP BY",
    "ORDER BY",
    "HAVING",
    "JOIN",
    "LEFT JOIN",
    "RIGHT JOIN",
    "FULL JOIN",
    "INNER JOIN",
    "CROSS JOIN",
    "NULL",
    "IS NULL",
    "IS NOT NULL",
    "CASE",
    "WHEN",
    "THEN",
    "ELSE",
    "END",
    "NOT",
    "IN",
    "BETWEEN",
    "LIMIT",
    "OFFSET",
    "DISTINCT",
    "UNION",
    "UNION ALL",
    "t",
    "a",
    "b",
    "c",
    "x",
    "y",
    "z",
    "employees",
    "departments",
    "id",
    "name",
    "(",
    ")",
    ",",
    ";",
    "*",
    "=",
    "<",
    ">",
    "<>",
    "+",
    "-",
    "||",
    "'abc'",
    "'x'",
    "42",
    "0",
    "1.5",
    "true",
    "false",
    "NVL",
    "DECODE",
    "SUBSTR",
    "MOD",
    "POSITION",
    "STRING_AGG",
    "ARRAY_AGG",
    "BOOL_AND",
    "BOOL_OR",
    "TO_DATE",
    "TO_CHAR",
    "TO_TIMESTAMP",
    "GENERATE_SERIES",
    "REGEXP_SUBSTR",
    "TRUNC",
    "CURRENT_TIMESTAMP",
    "SYSDATE",
    "COUNT",
    "SUM",
    "AVG",
    "MIN",
    "MAX",
    "COALESCE",
    "::",
    "::INT",
    "::BIGINT",
    "::JSONB",
    "::UUID",
    "::TIMESTAMPTZ",
    " ~ ",
    " ~* ",
    " !~ ",
    " !~* ",
    "START WITH",
    "CONNECT BY",
    "PRIOR",
    "(+)",
    "DISTINCT ON (k)",
];

/// Tiny xorshift PRNG — fully deterministic, no external crates.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn pick<'a, T>(&mut self, slice: &'a [T]) -> &'a T {
        let i = (self.next() as usize) % slice.len();
        &slice[i]
    }
    fn range(&mut self, lo: usize, hi: usize) -> usize {
        lo + (self.next() as usize) % (hi - lo + 1)
    }
}

fn gen_sql(rng: &mut Rng) -> String {
    let n = rng.range(1, 30);
    let mut parts = Vec::with_capacity(n);
    for _ in 0..n {
        parts.push(*rng.pick(FRAGMENTS));
    }
    parts.join(" ")
}

#[test]
fn translate_never_panics_on_random_input() {
    let mut rng = Rng(0xcafef00d_deadbeef);
    for _ in 0..5000 {
        let sql = gen_sql(&mut rng);
        // We only care that it doesn't panic. Errors are fine.
        let _ = translate(&sql);
    }
}

#[test]
fn successful_output_has_no_gaussdb_tokens() {
    let mut rng = Rng(0x1234_5678_9abc_def0);
    for _ in 0..5000 {
        let sql = gen_sql(&mut rng);
        let Ok(first) = translate(&sql) else {
            continue;
        };
        assert!(
            !first.contains("(+)"),
            "Oracle (+) leaked:\n  in:  {sql}\n  out: {first}"
        );
        // Space-bounded checks mirror the fuzz target's invariants.
        for bad in [" ~* ", " !~* ", " ~ ", " !~ "] {
            assert!(
                !first.contains(bad),
                "PG regex op {bad:?} leaked:\n  in:  {sql}\n  out: {first}"
            );
        }
        // `::` with a known GaussDB type attached should NEVER survive.
        for bad in ["::INT", "::BIGINT", "::JSONB", "::UUID"] {
            assert!(
                !first.contains(bad),
                "Typed :: cast {bad:?} leaked:\n  in:  {sql}\n  out: {first}"
            );
        }
    }
}

#[test]
fn idempotent_up_to_whitespace() {
    let mut rng = Rng(0xfeed_face_cafe_babe);
    let normalize = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ");
    for _ in 0..5000 {
        let sql = gen_sql(&mut rng);
        let Ok(first) = translate(&sql) else {
            continue;
        };
        let Ok(second) = translate(&first) else {
            continue;
        };
        assert_eq!(
            normalize(&first),
            normalize(&second),
            "non-idempotent:\n  in:  {sql}\n  out1: {first}\n  out2: {second}"
        );
    }
}

#[test]
fn smoke_samples_are_still_translated() {
    // Spot-check on known inputs to make sure the generator-driven tests
    // aren't passing because the translator has degenerated to identity.
    let cases = [
        ("SELECT NVL(x, 0) FROM t", "COALESCE(x, 0)"),
        ("SELECT x::JSONB FROM t", "CAST(x AS STRING)"),
        (
            "SELECT DECODE(x, 1, 'a') FROM t",
            "CASE WHEN x = 1 THEN 'a' END",
        ),
    ];
    for (input, want) in cases {
        let got = translate(input).unwrap();
        assert!(got.contains(want), "input: {input}, got: {got}");
    }
}
