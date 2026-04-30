# coral-rust

> 🌐 Languages: **English** | [简体中文](README.zh-CN.md)

Rust port of [`coral-gaussdb`](../coral-gaussdb) + [`coral-gaussdb-spark`](../coral-gaussdb-spark) — a GaussDB / openGauss SQL → Spark SQL translator. No JVM, single native binary, ~5ms per run.

## Status: POC ✅

Proof-of-concept that covers the same 6 representative samples as the Java tree's `SmokeDemo`. 15 tests green. This is intentionally *not* a full port of Coral — see **Scope** below.

## Why Rust

The Java `coral-gaussdb-spark` runs on top of Apache Calcite (18 MB shaded JAR, needs JVM, 1+ second cold start). For pipelines that only need **text → text** SQL translation (no query execution, no catalog lookup, no type coercion against real schemas), Calcite is overkill.

| Metric | Java `:coral-gaussdb-spark:smoke` | Rust `coral --smoke` |
|---|---|---|
| Cold start (6 samples) | ~1,266 ms | **~6 ms** |
| Dependencies | JVM + 15+ JARs (~50 MB) | 1 binary (~2 MB) |
| Deploy | `java -jar ...` | `./coral` |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│ 1. Text pre-processing (preprocess.rs)                      │
│    Rewrites Oracle-style START WITH / CONNECT BY into       │
│    standard WITH RECURSIVE — sqlparser-rs does not parse    │
│    Oracle hierarchical queries directly.                    │
├─────────────────────────────────────────────────────────────┤
│ 2. Parse (sqlparser-rs, PostgreSqlDialect)                  │
│    GaussDB is PostgreSQL-compatible, so PG parsing works    │
│    for the constructs coral-gaussdb handles.                │
├─────────────────────────────────────────────────────────────┤
│ 3. AST rewrite passes (rewrite/)                            │
│    - structure::DistinctOnRewriter                          │
│        DISTINCT ON (k) -> ROW_NUMBER() OVER subquery        │
│    - functions::FunctionRewriter                            │
│        NVL/NVL2/DECODE/SUBSTR/MOD/SYSDATE/RANDOM,           │
│        PG regex operators ~ ~* !~ !~*, :: cast              │
├─────────────────────────────────────────────────────────────┤
│ 4. Render (sqlparser-rs Display trait)                      │
│    Each AST node Display-formats itself as Spark-compatible │
│    SQL. No separate "Spark unparser" needed.                │
└─────────────────────────────────────────────────────────────┘
```

## Coverage vs Coral Java

| Sample (from SmokeDemo) | Java coral-gaussdb-spark | coral-rust |
|---|---|---|
| CTE + JOIN + window + NVL + `\|\|` | ✅ | ✅ `NVL → COALESCE` |
| `::INT` / DECODE / SUBSTR / MOD | ✅ | ✅ all 4 rewritten |
| `~*` / `~` regex operators | ✅ | ✅ `LOWER(x) RLIKE LOWER(p)` / `x RLIKE p` |
| MERGE INTO | ✅ | ✅ pass-through (syntax-compatible) |
| CONNECT BY recursive | ✅ | ✅ text preprocessor → WITH RECURSIVE |
| DISTINCT ON (k) | ✅ | ✅ ROW_NUMBER() subquery |

## Scope

**This POC deliberately handles only translation.** It does NOT replace Coral's:

- Catalog resolution (schema lookup against Hive Metastore) — coral-rust is pure text → text
- Type coercion across calls (Calcite's rich type system)
- UDF registry (StaticHiveFunctionRegistry's 100+ function mappings)
- `coral-hive`, `coral-trino`, `coral-spark`, `coral-incremental`, `coral-schema`, etc.

If you need any of the above, use the Java tree. If you just want to translate GaussDB SQL strings into Spark SQL strings in a native binary, this is the right tool.

## Usage

### CLI

```bash
# From stdin:
echo "SELECT NVL(x, 0), y::INT FROM t" | coral
# → SELECT COALESCE(x, 0), CAST(y AS INT) FROM t

# From a file:
coral --file query.sql

# Built-in demo (same 6 samples as :coral-gaussdb-spark:smoke):
coral --smoke
```

### Library

```toml
[dependencies]
coral-core = { path = "path/to/coral-rust/core" }
```

```rust
let spark_sql = coral_core::translate("SELECT DECODE(x, 1, 'a', 'b') FROM t")?;
// "SELECT CASE WHEN x = 1 THEN 'a' ELSE 'b' END FROM t"
```

## Build and test

```bash
cd coral-rust
cargo build --release        # target/release/coral
cargo test                   # 15 tests: 3 unit + 12 integration
cargo run --bin coral -- --smoke
```

## Extending

Adding a new function mapping is ~5 lines in `core/src/rewrite/functions.rs`:

```rust
"to_char" => {
    // Forward TO_CHAR(x, 'YYYY') to DATE_FORMAT(x, 'yyyy') with format translation
    // ...
}
```

Adding a new structural rewrite follows `DistinctOnRewriter` as a template: implement `VisitorMut`, override `post_visit_query`, rewire the `Query` node.

For CONNECT BY and other constructs sqlparser-rs cannot parse, extend `core/src/preprocess.rs` with a text-level transform.

## Limitations

- **MERGE INTO** pass-through assumes target table supports Spark MERGE (Delta / Iceberg / Hudi). Plain Hive tables will fail at execution time — this is a Spark limitation, not a translation issue.
- **Window frame clauses** (`ROWS BETWEEN ... AND ...`) are parsed but untested; they round-trip via `Display` which may or may not match the Coral Java output exactly.
- **CONNECT BY preprocessor** handles the canonical shape only (bare table in FROM, simple PRIOR equality). Complex cases fall through the parser and produce a clear parse error rather than silent wrong output.
- **Type system**: GaussDB `JSONB`, `UUID`, `INTERVAL` reach the output verbatim; Spark will complain if it does not understand them. A type-mapping layer is future work.

## License

BSD-2-Clause — same as the rest of the Coral project.
