# coral-cli

Command-line GaussDB / openGauss → Spark SQL translator.

Built on top of [`coral-core`](https://crates.io/crates/coral-core). Ships as a single static binary (~2 MB, no JVM, no runtime deps).

## Install

```bash
cargo install coral-cli
```

## Usage

```bash
# From stdin:
echo "SELECT NVL(x, 0) FROM t" | coral
# → SELECT COALESCE(x, 0) FROM t

# From a file:
coral --file query.sql

# Built-in demo (same 6 samples as the Java SmokeDemo):
coral --smoke
```

## See also

- [`coral-core`](https://crates.io/crates/coral-core) — the library this CLI wraps
- [`coral-ffi`](https://crates.io/crates/coral-ffi) — C ABI for non-Rust callers

## License

BSD-2-Clause
