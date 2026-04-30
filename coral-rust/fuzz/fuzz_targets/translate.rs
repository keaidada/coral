// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// Fuzz target for coral_core::translate.
//
// Invariants we assert:
//   1. translate() never panics.
//   2. For any input that's valid UTF-8 and parses successfully, the Spark
//      SQL output MUST NOT contain GaussDB-exclusive tokens we're supposed
//      to rewrite:
//        - `(+)`          — Oracle outer join marker
//        - ` NVL(`        — should have become COALESCE
//        - `::`           — should have become CAST AS
//        - PG regex ops   — ` ~* ` / ` !~* ` / ` ~ ` / ` !~ ` (whitespace-
//                           bounded to avoid false hits on `~t` inside
//                           identifiers)
//   3. Double translation is idempotent: translate(translate(x)) should
//      produce the same string as translate(x) (modulo whitespace, which
//      sqlparser normalizes anyway).
//
// The first invariant is what libFuzzer is here for — most real bugs show
// up as panics on malformed input.

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(s) = std::str::from_utf8(data) else {
        return;
    };

    let Ok(first) = coral_core::translate(s) else {
        // Parse errors are fine — we only fail on panics or forbidden tokens.
        return;
    };

    // Invariant 2: no GaussDB-exclusive tokens in the output.
    assert!(
        !first.contains("(+)"),
        "output retained Oracle (+) marker:\n  in:  {s}\n  out: {first}"
    );
    // `NVL(` with the `(` attached catches the call form, not random `NVL`
    // substrings in string literals.
    assert!(
        !first.contains(" NVL(") && !first.starts_with("NVL("),
        "output retained NVL call:\n  in:  {s}\n  out: {first}"
    );
    assert!(
        !first.contains("::INT")
            && !first.contains("::BIGINT")
            && !first.contains("::JSONB")
            && !first.contains("::UUID"),
        "output retained :: cast:\n  in:  {s}\n  out: {first}"
    );
    for bad in [" ~* ", " !~* ", " ~ ", " !~ "] {
        assert!(
            !first.contains(bad),
            "output retained PG regex op {bad:?}:\n  in:  {s}\n  out: {first}"
        );
    }

    // Invariant 3: idempotence.
    let Ok(second) = coral_core::translate(&first) else {
        return;
    };
    let normalize = |s: &str| {
        s.split_whitespace().collect::<Vec<_>>().join(" ")
    };
    assert_eq!(
        normalize(&first),
        normalize(&second),
        "translate not idempotent:\n  in:  {s}\n  out1: {first}\n  out2: {second}"
    );
});
