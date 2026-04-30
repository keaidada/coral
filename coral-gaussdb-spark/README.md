# coral-gaussdb-spark

> 🌐 Languages: **English** | [简体中文](README.zh-CN.md)

End-to-end GaussDB / openGauss SQL → Spark SQL translator. Wraps
[`coral-gaussdb`](../coral-gaussdb) (frontend) and
[`coral-spark`](../coral-spark) (backend) into a one-liner.

## Usage

### With an in-memory catalog (no Hive Metastore required)

Best for tests, CLI tools, quick experiments. The catalog is a nested map
`{dbName: {tableName: ["col|hiveType", ...]}}`.

```java
import com.linkedin.coral.gaussdb.spark.CoralGaussDBToSpark;
import java.util.*;

Map<String, Map<String, List<String>>> catalog = new HashMap<>();
Map<String, List<String>> defaultSchema = new HashMap<>();
defaultSchema.put("employees",
    Arrays.asList("id|int", "dept_id|int", "name|string", "salary|double"));
catalog.put("default", defaultSchema);

String sparkSql = CoralGaussDBToSpark.createLocal(
    "SELECT dept_id, COUNT(*) FROM employees GROUP BY dept_id HAVING COUNT(*) > 1",
    catalog
).getSparkSql();

System.out.println(sparkSql);
// → SELECT `employees`.`dept_id`, COUNT(*) FROM default.employees AS employees
//   GROUP BY `employees`.`dept_id` HAVING COUNT(*) > 1
```

### With a live Hive Metastore

For production pipelines alongside your existing Hive catalog:

```java
HiveMetastoreClient hms = new HiveMscAdapter(Hive.get(conf).getMSC());
CoralGaussDBToSpark result = CoralGaussDBToSpark.create(gaussdbSql, hms);

String   sparkSql   = result.getSparkSql();
RelNode  ir         = result.getRelNode();
List<String> deps   = result.getBaseTables();      // e.g. ["default.employees"]
List<SparkUDFInfo> udfs = result.getSparkUDFInfoList();

// Or the one-liner:
String sparkSql = CoralGaussDBToSpark.translate(gaussdbSql, hms);
```

## What gets translated

GaussDB → Spark SQL mapping highlights (full list:
[`../coral-gaussdb/docs/GRAMMAR_COVERAGE.md`](../coral-gaussdb/docs/GRAMMAR_COVERAGE.md)):

| Input (GaussDB) | Output (Spark) |
|---|---|
| `a \|\| b` | `concat(a, b)` or `a \|\| b` (Spark native) |
| `x::INT` | `CAST(x AS INT)` |
| `NVL(a, b)` | `COALESCE(a, b)` or `CASE WHEN a IS NOT NULL THEN a ELSE b END` |
| `DECODE(x, k1, v1, k2, v2, d)` | `CASE WHEN x = k1 THEN v1 WHEN x = k2 THEN v2 ELSE d END` |
| `COUNT(*)`, `SUM(...)`, `ROW_NUMBER() OVER (...)` | unchanged |
| `MOD(a, b)` | `a % b` |
| `STRING_AGG(x, sep)` | `concat_ws(sep, collect_list(x))` |
| `ARRAY_AGG(x)` | `collect_list(x)` |
| `REGEXP_SUBSTR(s, p)` | `regexp_extract(s, p, 0)` |
| `/*+ hint */` | stripped (Spark does not understand Oracle-style hints) |
| `START WITH ... CONNECT BY ...` | `WITH RECURSIVE __coral_connect_by AS (...)` (AST rewrite) |

## Limitations (v1)

- `MERGE INTO` and `CONNECT BY` produce correct SqlNode ASTs but the RelNode
  pipeline for them (and `WITH RECURSIVE`) is tracked as a Stage-4 item.
  Parsing and round-tripping through `toSqlNode` is fully supported today.
- Oracle-style outer join `(+)` is not supported — use ANSI JOIN.
- Window frames (`ROWS BETWEEN … AND …`) are not yet parsed.
- `to_date` / `to_char` format tokens (`YYYY` vs `yyyy`, `HH24` vs `HH`, …) pass
  through unchanged; a Spark-dialect translator is on the Stage-4 punch-list.

## Related modules

- [`coral-gaussdb`](../coral-gaussdb): frontend (GaussDB → RelNode IR)
- [`coral-spark`](../coral-spark): backend (RelNode → Spark SQL). Used via
  `CoralSpark.create(rel, hms)` under the hood.

## Running tests

```bash
./gradlew :coral-gaussdb-spark:test
```
