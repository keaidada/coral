# Coral Rust

> Languages: **English** | [简体中文](README.zh-CN.md)

This branch is the Rust-first Coral workspace. The former Java / Gradle backend has been removed; the active backend is `coral-rust/`, and the web UI is `coral-service/frontend/`.

## Layout

| Path | Purpose |
|---|---|
| `coral-rust/` | Rust workspace: SQL translation core, HTTP service, CLI, FFI, schema, visualization, incremental, Spark/Trino/Pig helpers |
| `coral-rust/service/` | Axum HTTP backend compatible with the Coral Service frontend |
| `coral-rust/cli/` | Local SQL translation CLI without starting the HTTP server; binary name is `coral` |
| `coral-service/frontend/` | Next.js frontend for translate, validate, visualize, function registry, settings, and local history |

## Portability / prerequisites

### Running prebuilt binaries

If you use prebuilt `coral` or `coral-service` binaries, the only requirement is a matching target OS / architecture:

- macOS / Linux / Windows binary for the target machine
- No JVM required
- No Gradle required
- No database service required
- CLI SQL translation does not require the HTTP server

### Building Rust backend / CLI from source

Required:

- Rust toolchain: `>= 1.75`
- Cargo
- On macOS, Xcode Command Line Tools: `xcode-select --install`

Install Rust:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

### Running the frontend

Required:

- Node.js: recommended `20.x`
- npm
- A modern browser

The frontend is only the UI and should point at a running Rust service, defaulting to `http://localhost:8080`.

### Optional capabilities

- Python wheel / Python FFI: Python 3
- C/Go/Node FFI integration: the relevant language toolchain plus `coral-rust/ffi` artifacts

## CLI: translate SQL without starting server

Yes. The CLI lives in `coral-rust/cli`; the crate is `coral-cli`, and the generated command is `coral`.

Run from source:

```bash
cd coral-rust
echo "SELECT NVL(x, 0), y::INT FROM t" | cargo run -p coral-cli -- --source gaussdb --target spark --pretty
```

Build release binary:

```bash
cd coral-rust
cargo build --release -p coral-cli
./target/release/coral --file query.sql --source gaussdb --target spark --pretty
```

Common commands:

```bash
# stdin input, Spark SQL output by default
echo "SELECT NVL(x, 0) FROM t" | ./target/release/coral --source gaussdb --target spark

# Aliases are available too
cat query.sql | ./target/release/coral --from hive --to trino

# Trino input to Spark output
cat trino.sql | ./target/release/coral --source trino --target spark --pretty

# File input
./target/release/coral --file query.sql --source gaussdb --target spark --pretty

# Unknown functions fail by default; specify a UDF allowlist for passthrough
echo "SELECT my_udf(x) FROM t" | ./target/release/coral --source gaussdb --target spark --allow-unknown-functions my_udf

# Multiple UDFs are comma-separated; omit the value to allow all unknown functions
echo "SELECT YNVL(x, 0), XNVL(x, 0) FROM t" | ./target/release/coral --from hive --to spark --allow-unknown-functions YNVL,XNVL

# Built-in smoke samples
./target/release/coral --smoke

# Function mapping table
./target/release/coral --list-functions
```

### Checking conversion success in shell scripts

`coral` follows standard Unix conventions:

- Exit code `0`: conversion succeeded; translated SQL is written to `stdout`
- Non-zero exit code: conversion failed; the error message is written to `stderr`
- Argument errors, SQL parse errors, and disallowed unknown functions all return non-zero

Recommended script:

```bash
#!/usr/bin/env bash
set -euo pipefail

CORAL="./coral-linux-x86_64-musl"
INPUT_FILE="input.sql"
OUTPUT_FILE="output.sql"
ERROR_FILE="coral-error.log"

if "$CORAL" --file "$INPUT_FILE" --from gaussdb --to spark > "$OUTPUT_FILE" 2> "$ERROR_FILE"; then
  echo "SQL conversion succeeded: $OUTPUT_FILE"
else
  echo "SQL conversion failed" >&2
  cat "$ERROR_FILE" >&2
  exit 1
fi
```

For business UDF passthrough, prefer an explicit allowlist:

```bash
"$CORAL" \
  --file "$INPUT_FILE" \
  --from hive \
  --to spark \
  --allow-unknown-functions YNVL,XNVL \
  > "$OUTPUT_FILE" \
  2> "$ERROR_FILE"
```

## Backend: Rust service

```bash
cd coral-rust
cargo run -p coral-service
```

Default bind address: `0.0.0.0:8080`.

Use a custom bind address:

```bash
CORAL_BIND=127.0.0.1:9000 cargo run -p coral-service
```

Health check:

```bash
curl http://127.0.0.1:8080/api/health
```

## Frontend

```bash
cd coral-service/frontend
npm install
cp .env.local.example .env.local
npm run dev
```

Set `.env.local` to point at the Rust backend:

```bash
NEXT_PUBLIC_CORAL_SERVICE_API_URL=http://localhost:8080
```

Open `http://localhost:3000`.

## Useful commands

Rust:

```bash
cd coral-rust
cargo fmt --all
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
cargo run --release --bin coral -- --smoke
```

Frontend:

```bash
cd coral-service/frontend
npm run lint
npm run build
```

## HTTP API

The Rust service exposes the endpoints used by the frontend:

| Method | Path | Purpose |
|---|---|---|
| `GET` | `/api/health` | Liveness check |
| `POST` | `/api/translations/translate` | Translate SQL between supported dialects |
| `POST` | `/api/translations/validate` | Parse / validate SQL |
| `POST` | `/api/visualizations/generategraphs` | Generate DOT / PlantUML graph source |
| `GET` | `/api/visualizations/{id}` | Retrieve generated graph source |
| `GET` | `/api/functions` | Function registry |
| `POST` | `/api/catalog-ops/execute` | Frontend compatibility endpoint for `CREATE DATABASE/TABLE/VIEW` |

## Notes

- No JVM or Gradle backend is required.
- The frontend local history is stored in browser `localStorage`.
- `coral-service/frontend/.next/`, `node_modules/`, and `coral-rust/target/` are build artifacts and should not be committed.
