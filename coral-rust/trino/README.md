# coral-trino

Trino SQL output backend for [coral-rust](../core) — the Rust port of LinkedIn
Coral's `HiveToTrinoConverter`.

## Install

```toml
[dependencies]
coral-trino = "0.1"
```

## Use

```rust
use coral_trino::to_trino_sql;

fn main() -> coral_trino::Result<()> {
    let hive = r#"
        SELECT
            NVL(name, 'n/a')                AS safe_name,
            RAND()                          AS nonce,
            DATE_ADD(created, 30)           AS renews_on,
            GET_JSON_OBJECT(payload, '$.k') AS k,
            ARRAY_CONTAINS(tags, 'priority')
        FROM events
    "#;

    println!("{}", to_trino_sql(hive)?);
    Ok(())
}
```

Produces (whitespace-normalized):

```sql
SELECT
    COALESCE(name, 'n/a')                          AS safe_name,
    RANDOM()                                       AS nonce,
    DATE_ADD('day', 30, CAST(created AS DATE))     AS renews_on,
    JSON_EXTRACT(payload, '$.k')                   AS k,
    CONTAINS(tags, 'priority')
FROM events
```

## What it rewrites

Everything `coral-core` rewrites for Spark, **plus** the Spark↔Trino diff:

- `RAND()` / `RAND_INTEGER(n)` → `RANDOM()`
- `GET_JSON_OBJECT(j, p)` → `JSON_EXTRACT(j, p)`
- `ARRAY_CONTAINS(arr, v)` → `CONTAINS(arr, v)`
- `BASE64` / `UNBASE64` / `HEX` / `UNHEX` → `TO_BASE64` / `FROM_BASE64` / `TO_HEX` / `FROM_HEX`
- `INSTR` → `STRPOS`
- `RLIKE` operator → `REGEXP_LIKE` function
- `COLLECT_LIST(x)` → `ARRAY_AGG(x)`, `COLLECT_SET(x)` → `ARRAY_AGG(DISTINCT x)`
- `PMOD(a, b)` → `((a % b) + b) % b`
- `DATE_ADD(d, n)` → `DATE_ADD('day', n, CAST(d AS DATE))`
- `DATE_SUB(d, n)` → `DATE_ADD('day', -n, CAST(d AS DATE))`
- `DATEDIFF(a, b)` → `DATE_DIFF('day', CAST(b AS DATE), CAST(a AS DATE))`
- `TO_DATE(s)` → `DATE(CAST(s AS TIMESTAMP))`
- `FLOAT` → `REAL`, `BINARY` → `VARBINARY`, `STRING` → `VARCHAR`

Unknown functions pass through — Trino will reject anything it genuinely
doesn't recognize; we don't invent bindings.

## See also

- [coral-core](../core) — the shared parse + rewrite pipeline
- [coral-rust root README](../README.md)
