# coral-fuzz

libFuzzer harness for `coral_core::translate`, exercised via [cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

## Prerequisites

```bash
rustup toolchain install nightly
cargo install cargo-fuzz
```

## Run

```bash
cd coral-rust/fuzz
cargo +nightly fuzz run translate
```

This runs libFuzzer against `fuzz_targets/translate.rs` with the seed corpus
in `corpus/translate/` (10 SQL samples covering the SmokeDemo + Stage 1-3
rewrites). libFuzzer will keep running indefinitely; Ctrl-C to stop.

## Invariants asserted by the target

1. `translate()` must never panic on any UTF-8 input.
2. When translation succeeds, the output must NOT contain these
   GaussDB-exclusive tokens that the rewriter is responsible for
   eliminating:
   - `(+)` (Oracle outer join marker)
   - ` NVL(` (should be COALESCE)
   - `::INT` / `::BIGINT` / `::JSONB` / `::UUID` (should be CAST AS)
   - Space-bounded PG regex ops: ` ~* `, ` !~* `, ` ~ `, ` !~ `
3. `translate(translate(x))` == `translate(x)` modulo whitespace
   (idempotence).

Any new rewrite rule should add its "before" tokens to the forbidden list
here so regressions surface immediately.

## Reproducing a crash

cargo-fuzz writes crashing inputs to `artifacts/translate/crash-<hash>`. To
reproduce:

```bash
cargo +nightly fuzz run translate artifacts/translate/crash-<hash>
```

## Related: stable property tests

The `core/tests/property.rs` integration tests run on stable Rust and assert
the same invariants against ~15,000 randomly-generated inputs per test run.
That's the continuous-integration layer; cargo-fuzz is the "run overnight to
find the weird ones" layer.
