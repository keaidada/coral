# coral-schema

Avro schema inference for SQL views — Rust port of LinkedIn Coral's
`coral-schema` module, specifically the `ViewToAvroSchemaConverter` entry point.

## Install

```toml
[dependencies]
coral-schema = "0.1"
coral-core   = "0.1"
```

## Use

```rust
use coral_core::InMemoryCatalog;
use coral_schema::to_avro_schema;

fn main() {
    let catalog = InMemoryCatalog::from_pairs(&[
        ("hr", "employees", &[
            "id|BIGINT",
            "name|VARCHAR",
            "salary|DOUBLE",
            "hired|DATE",
            "tags|ARRAY<STRING>",
        ]),
    ]);

    let json = to_avro_schema(
        "CREATE VIEW active AS SELECT id, name, salary, hired, tags FROM hr.employees",
        &catalog,
    ).unwrap();

    println!("{json}");
}
```

Output (pretty-printed):

```json
{
  "type": "record",
  "name": "active",
  "fields": [
    {"name":"id",     "type":["null","long"]},
    {"name":"name",   "type":["null","string"]},
    {"name":"salary", "type":["null","double"]},
    {"name":"hired",  "type":["null",{"type":"int","logicalType":"date"}]},
    {"name":"tags",   "type":["null",{"type":"array","items":"string"}]}
  ]
}
```

## How it works

```
SQL text
   ↓ sqlparser-rs (PostgreSQL dialect)
CREATE VIEW ... AS SELECT ...
   ↓ extract_view
(view_name, Query)
   ↓ unwrap_select + build_scope
Select body + FROM aliases
   ↓ per-projection type inference
AvroRecord
   ↓ serde
Avro JSON schema
```

### Type inference walks the projection expressions

- **Column refs** (`emp.name`) resolve via the catalog's Hive type string
  (`"BIGINT"`, `"ARRAY<STRING>"`, `"STRUCT<a:INT,b:STRING>"`, …) and map to
  the matching Avro type.
- **Literals** → type from literal kind (int → `long`, float → `double`,
  string → `string`, bool → `boolean`).
- **Arithmetic** (`a + b`) → widen-to-widest (`int + double` → `double`).
- **String ops** (`a || b`, `CONCAT`, `SUBSTRING`, …) → `string`.
- **Aggregates** — `COUNT` → `long`, `SUM`/`AVG` → `double`, `MIN`/`MAX` →
  per-arg.
- **Date/time ops** — `YEAR`/`MONTH`/`DAY` → `int`, `TO_DATE` → `date`,
  `CURRENT_TIMESTAMP` → `timestamp-millis`.
- **Unknown function** → nullable `string` (permissive fallback, same as
  the Java tree does for unregistered UDFs).

All fields are emitted as nullable unions (`["null", T]`), which matches
what `ViewToAvroSchemaConverter` emits by default (the Java tree's
stricter "only-nullable-if-input-nullable" rule requires full RelNode
type information, which sqlparser-rs doesn't carry).

## Implementation trade-offs vs Java coral-schema

| Feature | Java | coral-rust | Notes |
|---|---|---|---|
| `CREATE VIEW` DDL | ✅ | ✅ | full support |
| Plain `SELECT` | ✅ | ✅ | auto-named `view` |
| Flat columns | ✅ | ✅ | full |
| Nested `STRUCT<...>` columns | ✅ | ✅ | string-parsed from catalog |
| Nested field access `a.b.c` | ✅ | ⚠️ | only 2-level (`alias.col`); deep walks fall back to `string` |
| `ARRAY<...>` / `MAP<...>` | ✅ | ✅ | full |
| Aggregates | ✅ | ✅ | 30+ function return types known |
| UDFs (unknown) | strict | lenient | Java errors; Rust emits nullable `string` |
| `UNION ALL` | ✅ | ⚠️ | uses left branch's layout |
| `forceLowercase` mode | ✅ | — | not ported |
| Namespace / `strictMode` | ✅ | — | not ported |

Full coverage matches the practical "flat projection" view case, which
is the bulk of what Java coral-schema handles in production.

## Tests

```bash
cargo test -p coral-schema
# 15 smoke tests (arithmetic / aggregates / complex types / joins / json shape).
```

## See also

- [coral-core](../core) — parse + rewrite pipeline
- [coral-hive-catalog](../hive_catalog) — live Hive Metastore catalog client
