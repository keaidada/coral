# coral-core

Rust library for translating GaussDB / openGauss SQL into Spark SQL.

Port of the translation core in [coral-gaussdb](https://github.com/linkedin/coral/tree/master/coral-gaussdb) + [coral-gaussdb-spark](https://github.com/linkedin/coral/tree/master/coral-gaussdb-spark) — no JVM, no Calcite, pure text → text.

## Usage

```rust
use coral_core::translate;

let spark_sql = translate("SELECT NVL(x, 0), y::JSONB FROM t")?;
// "SELECT COALESCE(x, 0), CAST(y AS STRING) FROM t"
```

With catalog-aware typo detection:

```rust
use coral_core::{translate_with_catalog, InMemoryCatalog};

let cat = InMemoryCatalog::from_pairs(&[
    ("default", "employees", &["id|int", "name|string", "dept_id|int"]),
]);
let r = translate_with_catalog("SELECT e.dpt_id FROM default.employees e", &cat)?;
// r.issues[0] -> UnknownColumn { did_you_mean: Some("dept_id") }
```

## Coverage

- 30 function mappings (NVL / DECODE / SUBSTR / MOD / `::` cast / PG regex / date format tokens)
- GaussDB type mapping (JSONB/UUID/BYTEA/TIMESTAMPTZ/Int2-4-8/Float4-8/SERIAL family)
- Structural rewrites: DISTINCT ON → ROW_NUMBER, CONNECT BY → WITH RECURSIVE, Oracle `(+)` → LEFT JOIN

See the full README in the [workspace root](https://github.com/keaidada/coral/tree/coral-rust/coral-rust).

## Related crates

- [`coral-cli`](https://crates.io/crates/coral-cli) — command-line wrapper around this library
- [`coral-ffi`](https://crates.io/crates/coral-ffi) — C ABI for Python / Go / Node bindings

## License

BSD-2-Clause
