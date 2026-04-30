# coral-gaussdb

> 🌐 Languages: **English** | [简体中文](README.zh-CN.md)

GaussDB / openGauss SQL → Coral RelNode (Apache Calcite relational algebra) frontend.

Mirrors the design of [`coral-trino`](../coral-trino): own ANTLR4 parser →
[`ParseTreeBuilder`](src/main/java/com/linkedin/coral/gaussdb/parsetree/ParseTreeBuilder.java)
(AST visitor) → Calcite `SqlNode` → `HiveSqlValidator` → `RelNode`.

> **For end-to-end GaussDB → Spark SQL translation, use
> [`coral-gaussdb-spark`](../coral-gaussdb-spark). This module only covers the
> frontend (GaussDB → RelNode IR).**

## Usage

```java
import com.linkedin.coral.gaussdb.GaussDBToRelConverter;
import org.apache.calcite.rel.RelNode;

// With a live Hive metastore:
GaussDBToRelConverter converter = new GaussDBToRelConverter(hiveMetastoreClient);
RelNode rel = converter.convertSql(
    "SELECT dept_id, COUNT(*) FROM employees GROUP BY dept_id");

// Or with an in-memory catalog for tests / dry runs:
Map<String, Map<String, List<String>>> catalog = new HashMap<>();
catalog.put("default", Collections.singletonMap(
    "employees", Arrays.asList("id|int", "dept_id|int", "name|string")));
RelNode rel = new GaussDBToRelConverter(catalog)
    .convertSql("SELECT ...");
```

## Scope

See [`docs/GRAMMAR_COVERAGE.md`](docs/GRAMMAR_COVERAGE.md) for the authoritative
rule / test / state table. At a glance:

- **Stage 1**: SELECT with WHERE, ORDER BY, LIMIT/OFFSET, INNER/LEFT/RIGHT JOIN,
  CASE, COALESCE/NVL/NVL2, string concat `||`, PG `::` cast, CAST, function
  calls, literals, double-quoted identifiers.
- **Stage 2**: GROUP BY / HAVING, UNION/INTERSECT/EXCEPT/MINUS, non-recursive
  CTEs, scalar / IN / EXISTS subqueries, FULL OUTER / CROSS / USING joins,
  window functions `OVER (PARTITION BY ... ORDER BY ...)`, INSERT … SELECT.
- **Stage 3**: optimizer hints (discarded), DELETE, UPDATE, MERGE INTO, CONNECT
  BY (rewritten to recursive CTE), GaussDB-specific types (BYTEA / UUID / JSON /
  JSONB / TIMESTAMPTZ / INTERVAL).
- **Function mapping**: `decode` → CASE, `sysdate/now()` → CURRENT_TIMESTAMP,
  `substr` → SUBSTRING, `nvl/nvl2` → COALESCE/CASE, `mod` → `%`, `random` →
  `rand`, `array_agg` → `collect_list`, `string_agg` → `concat_ws(collect_list)`,
  `trunc` → `date_trunc`, `regexp_substr` → `regexp_extract`, and ~30 others.

## Policy: hard-fail on unknown

Per the approved plan: any grammar node or function name we do not recognize
raises `UnhandledASTNodeException` (grammar-accepted but un-visited nodes) or
falls back to `SqlUnresolvedFunction` (unknown names — the validator then
decides). We never silently pass through unknown syntax.

## Developing

```bash
# Generate parser (runs automatically on compile):
./gradlew :coral-gaussdb:generateGrammarSource

# Full test suite:
./gradlew :coral-gaussdb:test

# Add a new feature:
#   1. Extend src/main/antlr/GaussDBSql.g4
#   2. Add a visit* method in ParseTreeBuilder.java
#   3. Add a test in GaussDBToRelConverterTest.java
#   4. Update docs/GRAMMAR_COVERAGE.md
```

> **Note**: `./gradlew spotlessCheck` on this module pulls Eclipse formatter
> JARs from jcenter, which is deprecated and may fail in restricted networks.
> All sources follow Coral's standard conventions manually: BSD-2 license
> header, import ordering `java / javax / com / org / com.linkedin.coral / #`,
> and 2-space indentation.

## Architecture invariants

- **Reuse Hive's registry**: function resolution chains `SqlStdOperatorTable +
  DaliOperatorTable(StaticHiveFunctionRegistry)`. GaussDB-specific rewrites
  happen **at AST time** in `ParseTreeBuilder`, keeping the IR dialect-neutral.
- **PG case folding**: unquoted identifiers fold to lowercase; `"Foo"` preserves
  case via `QUOTED_IDENTIFIER`.
- **Aggregate functions emit std ops directly** (`SUM/COUNT/AVG/...`) — falling
  back to `SqlUnresolvedFunction(USER_DEFINED_FUNCTION)` trips Calcite's
  type-coercion AssertionError.
- **ANTLR 4.9.3 pinned** — the last JDK-8-compatible version, matching Coral's
  build target.
