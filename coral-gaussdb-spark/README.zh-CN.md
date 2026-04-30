# coral-gaussdb-spark

> 🌐 语言版本：[English](README.md) | **简体中文**

端到端的 GaussDB / openGauss SQL → Spark SQL 翻译器。把
[`coral-gaussdb`](../coral-gaussdb)（前端）和
[`coral-spark`](../coral-spark)（后端）封装成一行代码即可调用的形式。

## 使用方式

### 使用内存目录（无需 Hive Metastore）

最适合测试、CLI 工具、临时实验。目录结构是一个嵌套 map：
`{dbName: {tableName: ["col|hiveType", ...]}}`。

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

### 使用在线 Hive Metastore

适合与现有 Hive 目录共用的生产数据管道：

```java
HiveMetastoreClient hms = new HiveMscAdapter(Hive.get(conf).getMSC());
CoralGaussDBToSpark result = CoralGaussDBToSpark.create(gaussdbSql, hms);

String   sparkSql   = result.getSparkSql();
RelNode  ir         = result.getRelNode();
List<String> deps   = result.getBaseTables();      // 例如 ["default.employees"]
List<SparkUDFInfo> udfs = result.getSparkUDFInfoList();

// 或者使用一行式的快捷方法：
String sparkSql = CoralGaussDBToSpark.translate(gaussdbSql, hms);
```

## 会被翻译的内容

GaussDB → Spark SQL 的重点映射（完整列表见
[`../coral-gaussdb/docs/GRAMMAR_COVERAGE.md`](../coral-gaussdb/docs/GRAMMAR_COVERAGE.md)）：

| 输入（GaussDB） | 输出（Spark） |
|---|---|
| `a \|\| b` | `concat(a, b)` 或 `a \|\| b`（Spark 原生） |
| `x::INT` | `CAST(x AS INT)` |
| `NVL(a, b)` | `COALESCE(a, b)` 或 `CASE WHEN a IS NOT NULL THEN a ELSE b END` |
| `DECODE(x, k1, v1, k2, v2, d)` | `CASE WHEN x = k1 THEN v1 WHEN x = k2 THEN v2 ELSE d END` |
| `COUNT(*)`、`SUM(...)`、`ROW_NUMBER() OVER (...)` | 保持不变 |
| `MOD(a, b)` | `a % b` |
| `STRING_AGG(x, sep)` | `concat_ws(sep, collect_list(x))` |
| `ARRAY_AGG(x)` | `collect_list(x)` |
| `REGEXP_SUBSTR(s, p)` | `regexp_extract(s, p, 0)` |
| `/*+ hint */` | 直接丢弃（Spark 不识别 Oracle 风格 hint） |
| `START WITH ... CONNECT BY ...` | `WITH RECURSIVE __coral_connect_by AS (...)`（AST 重写） |

## 当前限制（v1）

- `MERGE INTO` 和 `CONNECT BY` 能生成正确的 SqlNode AST，但其 RelNode 管道
  （以及 `WITH RECURSIVE`）仍作为 Stage-4 工作项跟进。目前已完整支持解析以及
  经由 `toSqlNode` 的回写。
- 不支持 Oracle 风格的 `(+)` 外连接写法 —— 请改用 ANSI JOIN。
- 窗口帧（`ROWS BETWEEN … AND …`）尚未支持解析。
- `to_date` / `to_char` 的格式 token（`YYYY` vs `yyyy`、`HH24` vs `HH`……）
  目前原样透传；面向 Spark 方言的转换器已列入 Stage-4 的待办事项。

## 相关模块

- [`coral-gaussdb`](../coral-gaussdb)：前端（GaussDB → RelNode IR）
- [`coral-spark`](../coral-spark)：后端（RelNode → Spark SQL）。底层通过
  `CoralSpark.create(rel, hms)` 调用。

## 运行测试

```bash
./gradlew :coral-gaussdb-spark:test
```
