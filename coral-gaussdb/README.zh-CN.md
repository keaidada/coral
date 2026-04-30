# coral-gaussdb

> 🌐 语言版本：[English](README.md) | **简体中文**

GaussDB / openGauss SQL → Coral RelNode（Apache Calcite 关系代数）前端。

整体设计对齐 [`coral-trino`](../coral-trino)：自带的 ANTLR4 解析器 →
[`ParseTreeBuilder`](src/main/java/com/linkedin/coral/gaussdb/parsetree/ParseTreeBuilder.java)
（AST visitor）→ Calcite `SqlNode` → `HiveSqlValidator` → `RelNode`。

> **如需端到端的 GaussDB → Spark SQL 翻译，请使用
> [`coral-gaussdb-spark`](../coral-gaussdb-spark)。本模块只负责前端
>（GaussDB → RelNode IR）。**

## 使用方式

```java
import com.linkedin.coral.gaussdb.GaussDBToRelConverter;
import org.apache.calcite.rel.RelNode;

// 搭配在线 Hive metastore：
GaussDBToRelConverter converter = new GaussDBToRelConverter(hiveMetastoreClient);
RelNode rel = converter.convertSql(
    "SELECT dept_id, COUNT(*) FROM employees GROUP BY dept_id");

// 或者使用内存目录，适合测试 / dry run：
Map<String, Map<String, List<String>>> catalog = new HashMap<>();
catalog.put("default", Collections.singletonMap(
    "employees", Arrays.asList("id|int", "dept_id|int", "name|string")));
RelNode rel = new GaussDBToRelConverter(catalog)
    .convertSql("SELECT ...");
```

## 覆盖范围

权威的规则 / 测试 / 状态对照表见 [`docs/GRAMMAR_COVERAGE.md`](docs/GRAMMAR_COVERAGE.md)。概览如下：

- **Stage 1**：SELECT 以及 WHERE、ORDER BY、LIMIT/OFFSET、INNER/LEFT/RIGHT JOIN、
  CASE、COALESCE/NVL/NVL2、字符串拼接 `||`、PG 的 `::` 类型转换、CAST、
  函数调用、字面量、双引号标识符。
- **Stage 2**：GROUP BY / HAVING、UNION/INTERSECT/EXCEPT/MINUS、非递归 CTE、
  标量 / IN / EXISTS 子查询、FULL OUTER / CROSS / USING JOIN、窗口函数
  `OVER (PARTITION BY ... ORDER BY ...)`、INSERT … SELECT。
- **Stage 3**：优化器 hint（直接丢弃）、DELETE、UPDATE、MERGE INTO、CONNECT
  BY（会重写为递归 CTE）、GaussDB 特有类型（BYTEA / UUID / JSON /
  JSONB / TIMESTAMPTZ / INTERVAL）。
- **函数映射**：`decode` → CASE，`sysdate/now()` → CURRENT_TIMESTAMP，
  `substr` → SUBSTRING，`nvl/nvl2` → COALESCE/CASE，`mod` → `%`，`random` →
  `rand`，`array_agg` → `collect_list`，`string_agg` → `concat_ws(collect_list)`，
  `trunc` → `date_trunc`，`regexp_substr` → `regexp_extract`，以及另外约 30 个函数。

## 策略：遇到未知即硬失败

根据批准的设计方案：遇到任何无法识别的语法节点或函数名，都会抛出
`UnhandledASTNodeException`（语法被解析但没有对应 visitor 的情况），或回退到
`SqlUnresolvedFunction`（未知函数名 —— 由 validator 决定如何处理）。我们**不会**
默默放过任何未知语法。

## 开发

```bash
# 生成解析器（compile 阶段会自动执行）：
./gradlew :coral-gaussdb:generateGrammarSource

# 运行完整测试：
./gradlew :coral-gaussdb:test

# 新增功能的步骤：
#   1. 扩展 src/main/antlr/GaussDBSql.g4
#   2. 在 ParseTreeBuilder.java 中添加对应的 visit* 方法
#   3. 在 GaussDBToRelConverterTest.java 中补充测试
#   4. 更新 docs/GRAMMAR_COVERAGE.md
```

> **注意**：本模块的 `./gradlew spotlessCheck` 会从 jcenter 拉取 Eclipse
> formatter 的 JAR，而 jcenter 已被废弃，在受限网络下可能失败。本模块的源码均
> 手工遵循 Coral 的统一规范：BSD-2 许可头、`java / javax / com / org /
> com.linkedin.coral / #` 的 import 排序、2 个空格缩进。

## 架构约束

- **复用 Hive 的注册表**：函数解析走 `SqlStdOperatorTable +
  DaliOperatorTable(StaticHiveFunctionRegistry)` 的链路。GaussDB 专属的重写
  **在 AST 阶段** 完成于 `ParseTreeBuilder`，以保持 IR 对方言的中立。
- **PG 大小写折叠**：未加引号的标识符会折叠为小写；`"Foo"` 则通过
  `QUOTED_IDENTIFIER` 保留原始大小写。
- **聚合函数直接产出标准算子**（`SUM/COUNT/AVG/...`）—— 若回退到
  `SqlUnresolvedFunction(USER_DEFINED_FUNCTION)` 会触发 Calcite 的类型强制转换
  AssertionError。
- **ANTLR 固定在 4.9.3** —— 最后一个兼容 JDK 8 的版本，与 Coral 的构建目标一致。
