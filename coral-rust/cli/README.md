# coral-cli

Command-line GaussDB / openGauss / Hive-flavored SQL → Spark or Trino SQL translator.

Built on top of `coral-core`. Ships as a single native binary, no JVM, no Gradle, and no HTTP server required.

## Requirements

- To run a prebuilt binary: only a matching OS / architecture.
- To build from source: Rust `>= 1.75` and Cargo.
- On macOS, install Xcode Command Line Tools if the linker is missing: `xcode-select --install`.

## Install

```bash
cargo install coral-cli
```

## Usage

```bash
# Run from source, no server needed:
echo "SELECT NVL(x, 0) FROM t" | cargo run -p coral-cli -- --source gaussdb --target spark --pretty

# Build once, then use the native binary:
cargo build --release -p coral-cli
echo "SELECT NVL(x, 0) FROM t" | ./target/release/coral --source gaussdb --target spark
# → SELECT COALESCE(x, 0) FROM t

# From a file:
./target/release/coral --file query.sql --source gaussdb --target spark --pretty

# Aliases are available too:
cat query.sql | ./target/release/coral --from hive --to trino

# Trino input to Spark output:
cat trino.sql | ./target/release/coral --source trino --target spark

# Unknown functions fail by default; allow passthrough only for confirmed business UDFs:
echo "SELECT my_udf(x) FROM t" | ./target/release/coral --source gaussdb --target spark --allow-unknown-functions my_udf

# Multiple UDFs are comma-separated; omit the value to allow all unknown functions:
echo "SELECT YNVL(x, 0), XNVL(x, 0) FROM t" | ./target/release/coral --from hive --to spark --allow-unknown-functions YNVL,XNVL

# Built-in demo:
./target/release/coral --smoke

# Function mapping table:
./target/release/coral --list-functions
```

## Shell scripting

Use the process exit code to decide whether conversion succeeded:

- `0`: success, translated SQL on `stdout`
- non-zero: failure, error details on `stderr`

```bash
#!/usr/bin/env bash
set -euo pipefail

if ./coral --file input.sql --from gaussdb --to spark > output.sql 2> coral-error.log; then
  echo "SQL conversion succeeded"
else
  echo "SQL conversion failed" >&2
  cat coral-error.log >&2
  exit 1
fi
```

## See also

- [`coral-core`](https://crates.io/crates/coral-core) — the library this CLI wraps
- [`coral-ffi`](https://crates.io/crates/coral-ffi) — C ABI for non-Rust callers

## License

BSD-2-Clause
