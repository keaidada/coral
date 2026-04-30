# coral-incremental

Incremental materialized-view query rewriter — Rust port of Java
`coral-incremental`'s `RelNodeIncrementalTransformer`.

## What it does

Given a SQL query `Q` that reads from N base tables, produce the
"incremental form": a `UNION ALL` over every non-empty subset of base
tables replaced by their `_delta` sibling, such that unioning all of
them equals "everything that changed since the last run".

For one table:

```
SELECT id FROM a
```

becomes:

```
SELECT id FROM a_delta
```

For two tables (2^2 - 1 = 3 branches):

```
SELECT a.id FROM a JOIN b ON a.k = b.k
```

becomes:

```
SELECT a.id FROM a_delta JOIN b       ON a.k = b.k
UNION ALL
SELECT a.id FROM a       JOIN b_delta ON a.k = b.k
UNION ALL
SELECT a.id FROM a_delta JOIN b_delta ON a.k = b.k
```

For three tables (2^3 - 1 = 7 branches), and so on.

## Use

```rust
use coral_incremental::{incremental_sql, incremental_sql_with};

let out = incremental_sql("SELECT a.id FROM a JOIN b ON a.k = b.k").unwrap();
// 3 UNION ALL branches, each swapping a different subset of tables.

let custom = incremental_sql_with("SELECT id FROM t", "__incr").unwrap();
// -> SELECT id FROM t__incr
```

## Guards

- Caps at `MAX_TABLES = 4` (→ 15 branches). Beyond that, use a real
  CDC approach instead of combinatorial expansion — the Java tree
  has the same guard.
- `SELECT` with no table references returns `NoTables` error.
- Qualified names (`hr.employees`) get the suffix on the **last**
  segment → `hr.employees_delta`.

## Limitations vs Java

| Case | Java coral-incremental | coral-rust |
|---|---|---|
| Flat join | ✅ | ✅ |
| Union / Set-op | ✅ | ✅ (sqlparser walks into both branches) |
| Aggregate | ✅ propagates through child | ✅ same |
| Filter / Project | ✅ propagates | ✅ same |
| Subquery tables | ✅ | ✅ |
| CTE (WITH) | strict (CTE names are NOT expanded) | ⚠️ CTE names are treated as base tables. Inline CTEs before calling `incremental_sql` if you need strict semantics. |

## Tests

```bash
cargo test -p coral-incremental
# 11 smoke tests: 1/2/3 tables, custom suffix, qualified names,
# subqueries, CTE edge case, size guards, empty-input error.
```
