# coral-rust

> 🌐 Languages: **English** | [简体中文](README.zh-CN.md)

Rust port of **the full LinkedIn Coral stack** (not just `coral-gaussdb`). Twelve crates in one workspace cover the GaussDB/Hive → Spark/Trino/Pig translation surface, plus Avro schema inference, HTTP service, AST visualizer, incremental-materialized-view rewriter, and cross-platform FFI. No JVM, single native binary, ~6 ms per run.

## Status: Java-parity coverage

245 tests green across 12 crates. Stages G–O complete the remaining Java-Coral modules:

### Stages 1–5 (original GaussDB → Spark core)
- **Stage 1** — full GaussDB function registry (30 rules, including PG date-format-token translation)
- **Stage 2** — type-mapping layer (JSONB / UUID / BYTEA / TIMESTAMPTZ / Int2-4-8, Float4-8, SERIAL family)
- **Stage 3** — Oracle `(+)` outer joins, window frames, CONNECT BY
- **Stage 4** — optional catalog layer with typo-detection + "did you mean?" suggestions
- **Stage 5** — C FFI with Python + C example bindings

### Stages A–F (infra + full StaticHiveFunctionRegistry)
- **Stage A** — GitHub Actions CI (template under `ci/`)
- **Stage B** — crates.io + PyPI wheel packaging
- **Stage C** — real Hive Metastore HTTP catalog (`coral-hive-catalog`)
- **Stage D** — 198-entry function registry (registry-driven rewriter)
- **Stage E** — cargo-fuzz + stable-Rust property tests
- **Stage F** — commit pipeline consolidation

### Stages G–O (full Java-Coral parity)
- **Stage G** — Trino output backend (`coral-trino`, port of `coral-trino`)
- **Stage H** — HTTP service (`coral-service`, axum, port of `coral-service`)
- **Stage I** — function registry expanded to **331 entries** (≥302 Hive-parity target)
- **Stage J** — Avro schema inference (`coral-schema`, port of `coral-schema`)
- **Stage K** — Spark catalog/plan helpers (`coral-spark`, ports of `coral-spark-catalog` + `coral-spark-plan`)
- **Stage L** — AST visualization DOT/PlantUML (`coral-viz`, port of `coral-visualization`)
- **Stage M** — Incremental materialized views (`coral-incremental`, port of `coral-incremental`)
- **Stage N** — Pig Latin output (`coral-pig`, port of `coral-pig`)
- **Stage O** — cross-crate e2e tests + tagged release

### Workspace layout

| Crate | Maps to Java module |
|---|---|
| `core/` | `coral-gaussdb` + `coral-hive` translation core |
| `trino/` | `coral-trino` |
| `spark/` | `coral-spark-catalog` (partial) + `coral-spark-plan` (full) |
| `schema/` | `coral-schema` |
| `viz/` | `coral-visualization` |
| `incremental/` | `coral-incremental` |
| `pig/` | `coral-pig` |
| `service/` | `coral-service` |
| `hive_catalog/` | `coral-common` (HMS client subset) |
| `cli/` | — (new) |
| `ffi/` | — (new, C ABI + Python ctypes) |
| `e2e/` | — (new, cross-crate integration tests) |

## Why Rust

The Java `coral-gaussdb-spark` runs on Apache Calcite (18 MB shaded JAR, needs JVM, ~1.3 s cold start). For pipelines that only need **text → text** SQL translation (no query execution, no catalog lookup against real schemas, no runtime type coercion), Calcite is overkill.

| Metric | Java `:coral-gaussdb-spark:smoke` | Rust `coral --smoke` |
|---|---|---|
| Cold start (6 samples) | ~1,266 ms | **~6 ms** (~200× faster) |
| Dependencies | JVM + 15+ JARs (~50 MB) | 1 binary (~2 MB) |
| Deploy | `java -jar ...` | `./coral` |
| Call from Python/Go/Node | Gradle-built JAR + Py4J / JPype | `ctypes` / `cgo` / `ffi-napi` + .so |

## Architecture

```
┌──────────────────────────────────────────────────────────────────┐
│ 1. Text preprocessing — preprocess/                              │
│    Oracle START WITH / CONNECT BY     -> WITH RECURSIVE          │
│    Oracle `(+)` outer-join markers    -> LEFT JOIN ON            │
│    (sqlparser-rs cannot parse either of these natively)          │
├──────────────────────────────────────────────────────────────────┤
│ 2. Parse — sqlparser-rs PostgreSqlDialect                        │
│    GaussDB is PG-compatible; PG dialect covers the vast majority │
├──────────────────────────────────────────────────────────────────┤
│ 3. (Optional) Catalog validation — catalog::validate_against     │
│    UnknownTable / UnknownColumn warnings, Levenshtein "did you   │
│    mean?" suggestions, CTE-aware scoping                         │
├──────────────────────────────────────────────────────────────────┤
│ 4. AST rewrite passes — rewrite/                                 │
│    structure::DistinctOnRewriter    DISTINCT ON (k) -> ROW_NUMBER│
│    functions::FunctionRewriter      30 function/operator rules   │
│    types::TypeRewriter              GaussDB types -> Spark types │
├──────────────────────────────────────────────────────────────────┤
│ 5. Render — sqlparser-rs Display                                 │
│    AST nodes self-format as Spark-compatible SQL; no separate    │
│    "Spark unparser" needed                                       │
└──────────────────────────────────────────────────────────────────┘
```

## Function coverage

```
Aggregate/window:  COUNT, SUM, AVG, MIN, MAX (pass-through)
                   ROW_NUMBER, RANK, DENSE_RANK (pass-through)
                   BOOL_AND/BOOL_OR        -> EVERY/SOME
                   ARRAY_AGG               -> COLLECT_LIST
                   STRING_AGG(x, sep)      -> CONCAT_WS(sep, COLLECT_LIST(x))

Null handling:     COALESCE (pass-through)
                   NVL                     -> COALESCE
                   NVL2(a, b, c)           -> CASE WHEN a IS NOT NULL THEN b ELSE c END
                   DECODE(x, k1, v1, ...)  -> CASE WHEN x = k1 THEN v1 ... END

String/regex:      SUBSTR                  -> SUBSTRING
                   POSITION(a, b)          -> INSTR(b, a)
                   REGEXP_SUBSTR(s, p, …)  -> REGEXP_EXTRACT(s, p, 0)
                   x ~ p                   -> x RLIKE p
                   x ~* p                  -> LOWER(x) RLIKE LOWER(p)
                   x !~ p, x !~* p         -> NOT of above

Math:              MOD(a, b)               -> a % b
                   RANDOM                  -> RAND

Date/time:         SYSDATE, NOW            -> CURRENT_TIMESTAMP
                   TRUNC(d, 'MM')          -> DATE_TRUNC('MM', d)  (date form)
                   TO_CHAR(d, 'YYYY-MM')   -> DATE_FORMAT(d, 'yyyy-MM')
                   TO_DATE(s, 'YYYY-MM')   -> TO_DATE(s, 'yyyy-MM') (format translated)
                   TO_TIMESTAMP            -> same (format translated)
                   Date tokens: YYYY, YY, MON, MM, MI, DD, DY, HH24/HH12/HH,
                                SS, AM/PM, FF<n> all translated to Spark equivalents

Arrays/series:     GENERATE_SERIES(a, b)   -> SEQUENCE(a, b)
```

## Type coverage

```
JSON, JSONB, REGCLASS, TEXT              -> STRING
UUID                                     -> STRING
BYTEA                                    -> BINARY
TIMESTAMP WITH TIME ZONE / TIMESTAMPTZ   -> TIMESTAMP (drops timezone suffix)
INT2/INT4/INT8                           -> SMALLINT/INT/BIGINT
FLOAT4/FLOAT8                            -> REAL/DOUBLE
SMALLSERIAL/SERIAL/BIGSERIAL             -> SMALLINT/INT/BIGINT
INTERVAL, DATE, TIMESTAMP, DECIMAL, …    pass through
```

## Structural rewrites

| Input                                                  | Output                                                    |
|--------------------------------------------------------|-----------------------------------------------------------|
| `SELECT DISTINCT ON (k) ... ORDER BY ...`              | `SELECT ... FROM (SELECT ..., ROW_NUMBER() OVER (...) AS rn) WHERE rn = 1` |
| `SELECT ... START WITH ... CONNECT BY PRIOR id = pid`  | `WITH RECURSIVE __coral_connect_by AS (...) SELECT ...`    |
| `FROM a, b WHERE a.id = b.id(+)`                       | `FROM a LEFT JOIN b ON a.id = b.id`                       |

## Usage

### CLI

```bash
cargo build --release -p coral-cli

# From stdin:
echo "SELECT NVL(x, 0), y::INT FROM t" | ./target/release/coral
# → SELECT COALESCE(x, 0), CAST(y AS INT) FROM t

# From a file:
./target/release/coral --file query.sql

# Built-in demo (same 6 samples as :coral-gaussdb-spark:smoke):
./target/release/coral --smoke
```

### Library

```toml
[dependencies]
coral-core = { path = "path/to/coral-rust/core" }
```

```rust
// Plain translation
let spark_sql = coral_core::translate("SELECT DECODE(x, 1, 'a', 'b') FROM t")?;
// "SELECT CASE WHEN x = 1 THEN 'a' ELSE 'b' END FROM t"

// Catalog-aware
use coral_core::{InMemoryCatalog, translate_with_catalog};

let cat = InMemoryCatalog::from_pairs(&[
    ("default", "employees", &["id|int", "name|string", "dept_id|int"]),
]);
let r = translate_with_catalog("SELECT e.dpt_id FROM default.employees e", &cat)?;
// r.issues[0] -> UnknownColumn { column: "dpt_id", did_you_mean: Some("dept_id") }
// r.spark_sql -> translated SQL (returned regardless)
```

### Python

```bash
cargo build --release -p coral-ffi
python3 ffi/examples/python/smoke_demo.py
```

```python
from coral import translate, CoralError

try:
    spark_sql = translate("SELECT NVL(x, 0), y::INT FROM t")
    print(spark_sql)
except CoralError as e:
    print("translation failed:", e)
```

No PyO3. No build step. Just `ctypes` on the stdlib, loading `libcoral_ffi.dylib/.so/.dll`.

### C / C++

```bash
cargo build --release -p coral-ffi

cc ffi/examples/c/smoke.c -o smoke \
   -I ffi/include -L target/release -lcoral_ffi \
   -Wl,-rpath,'$ORIGIN/target/release'
./smoke
```

Header in `ffi/include/coral.h`; 4 functions total:
```c
char       *coral_translate(const char *input);    // returns owned, or NULL
void        coral_free_string(char *ptr);
const char *coral_last_error(void);                // thread-local, borrowed
const char *coral_version(void);                   // static, borrowed
```

## Build and test

```bash
cd coral-rust
cargo build --release
cargo test                       # 110 tests across 4 crates
cargo run --bin coral -- --smoke
```

## Not covered (yet)

This POC handles translation. It does NOT replace Coral's:

- Hive Metastore remote catalog (the trait is here, only the in-memory impl ships)
- Runtime UDF registry with Hive semantics (StaticHiveFunctionRegistry's 100+ rules)
- `coral-hive`, `coral-trino`, `coral-spark`, `coral-incremental`, `coral-schema`

If you need any of the above, use the Java tree. If you just want to translate GaussDB SQL strings into Spark SQL strings from Rust / Python / Go / Node / C, this is the right tool.

## License

BSD-2-Clause — same as the rest of the Coral project.
