# coral-spark

Spark-side helpers for [coral-rust](../core). Rust port of Java
`coral-spark-catalog` (the session-free subset) + `coral-spark-plan` (full
port).

## Install

```toml
[dependencies]
coral-spark = "0.1"
```

## What's ported, what's not

| Java module | Rust port | Notes |
|---|---|---|
| **coral-spark-catalog**: `CoralSparkViewCatalog.loadView()` | ⚠️ partial | `prepare_view()` returns the SparkSQL + Avro schema + referenced-table list. Session / UDF registration must happen in your Spark driver — we can't touch `SparkSession.active()` from Rust. |
| **coral-spark-plan**: `SparkPlanToIRRelConverter.getComplicatedPredicatePushedDownInfo()` | ✅ full | `analyze_plan()` + `classify_predicate()` are pure text parsers + one sqlparser call per predicate. No SparkSession, no Hive metastore needed. |

## Use

### Prepare a view for a Spark driver

```rust
use coral_core::InMemoryCatalog;
use coral_spark::prepare_view;

let catalog = InMemoryCatalog::from_pairs(&[
    ("hr", "employees", &["id|BIGINT", "name|VARCHAR", "salary|DOUBLE"]),
]);

let v = prepare_view(
    "CREATE VIEW active AS SELECT id, NVL(name, 'n/a') AS nm FROM hr.employees",
    &catalog,
).unwrap();

println!("{}", v.name);              // "active"
println!("{}", v.spark_sql);         // "SELECT id, COALESCE(name, 'n/a') ... "
println!("{}", v.avro_schema);       // {"type":"record","name":"active",...}
println!("{:?}", v.referenced_tables); // ["hr.employees"]
```

Pass `v` to your Spark driver (Scala/PySpark). The driver then calls
`spark.sql(v.spark_sql)` / `spark.catalog.createTable(v.name, ...)` using
the Avro schema you already have.

### Classify Spark plan predicates

```rust
use coral_spark::{analyze_plan, PredicateClass};

let plan = r"
    == Physical Plan ==
    *(1) Project [id#1, name#2]
    +- FileScan parquet hr.events[id#1,ts#2]
        PushedFilters: [IsNotNull(id), datediff(current_date(), ts) > 30]
        ReadSchema: struct<id:bigint,ts:timestamp>
";

for info in analyze_plan(plan) {
    println!("{}", info.table);  // "hr.events"
    for p in &info.predicates {
        println!("  {:?}  {}", p.class, p.predicate);
    }
}
```

Output:

```
hr.events
  Simple       IsNotNull(id)
  Complicated  datediff(current_date(), ts) > 30
```

Use this to flag jobs whose pushdowns wouldn't actually offload work to
the source.

### Classify one predicate

```rust
use coral_spark::{classify_predicate, PredicateClass};

assert_eq!(classify_predicate("x > 10"),           PredicateClass::Simple);
assert_eq!(classify_predicate("datediff(a, b) > 7"), PredicateClass::Complicated);
assert_eq!(classify_predicate(""),                  PredicateClass::Unparseable);
```

## Classification rules

| Expression kind | Verdict |
|---|---|
| `=`, `<>`, `<`, `<=`, `>`, `>=` | Simple |
| `AND`, `OR`, `NOT` | Simple (recurses) |
| `IN`, `BETWEEN`, `IS NULL`, `IS NOT NULL` | Simple |
| column reference, literal | Simple |
| Function call (any) | **Complicated** |
| `CASE WHEN ...` | **Complicated** |
| `CAST(...)` | **Complicated** |
| Subquery / `EXISTS` / `IN (subquery)` | **Complicated** |
| Unparseable text | Unparseable |

## Tests

```bash
cargo test -p coral-spark
# 15 smoke tests covering prepare_view + classify_predicate + analyze_plan.
```

## See also

- [coral-core](../core) — translation pipeline
- [coral-schema](../schema) — Avro schema inference used by `prepare_view`
