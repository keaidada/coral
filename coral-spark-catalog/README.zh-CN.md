# Coral Spark Catalog

> 🌐 语言版本：[English](README.md) | **简体中文**

一个面向 Spark 3.5 的 [CatalogExtension](https://spark.apache.org/docs/3.5.6/api/java/org/apache/spark/sql/connector/catalog/ViewCatalog.html)，通过拦截视图解析流程、借助 Coral 的翻译管道把视图定义翻译为 Spark SQL。视图可以用 Coral 支持的任意 SQL 方言定义（如 HiveQL、Trino SQL、Spark SQL）。这使得 Spark 可以透明地查询 Hive Metastore 中的视图，而无需手工重写 SQL。表、命名空间、函数等非视图操作仍会转发给底层的 session catalog 处理。

## 工作原理

当 Spark 需要解析一个视图时，`CoralSparkViewCatalog` 会拦截请求并：

1. 从 Hive Metastore 获取视图定义
2. 将视图转换为 Coral IR（例如对 HiveQL 视图使用 `HiveToRelConverter`）
3. 使用 `CoralSpark` 将 Coral IR 翻译为 Spark SQL
4. 通过 `ViewToAvroSchemaConverter` 基于 Avro 推导出视图 schema
5. 自动注册该视图依赖的所有 UDF（Hive、Spark 以及 Transport UDF）
6. 返回一个 Spark 可以原生执行的 `View` 对象

所有非视图操作（表、命名空间、函数）都会原封不动地转发给 session catalog。

## 配置

将 `CoralSparkViewCatalog` 注册为 Spark 的 session catalog：

```java
SparkSession spark = SparkSession.builder()
    .config("spark.sql.catalog.spark_catalog", CoralSparkViewCatalog.class.getName())
    .enableHiveSupport()
    .getOrCreate();
```

该 catalog 会通过标准的 Hive 配置（`hive-site.xml` 或 `HiveConf` 属性）发现 Hive Metastore 的连接信息。

## UDF 注册

`CoralSparkViewCatalog` 会自动注册视图所依赖的 UDF，共支持三种类型：

- **Hive 自定义 UDF**：通过 UDF 类名注册到 Spark 的 `SessionCatalog`
- **Spark UDF**：通过反射识别（拥有 `Seq<Expression>` 构造函数的类），并注册到 Spark 的 `FunctionRegistry`
- **Transport UDF**：通过调用 UDF 类上的静态 `register` 方法完成注册

UDF 所需的 JAR 依赖会依据 `CoralSpark` 提供的产物 URL 自动加载。

## 依赖

本模块依赖以下 Coral 模块：

- `coral-hive`：将 Hive 视图定义转换为 Coral IR
- `coral-spark`：将 Coral IR 翻译为 Spark SQL，并提供 UDF 元数据
- `coral-schema`：基于视图定义推导 Avro schema

Spark 3.5（`spark-sql` 与 `spark-avro`）仅作为 compile-only 依赖，预期由运行时环境提供。

## 测试

测试使用内嵌的 Hive Metastore（底层是内存中的 Derby 数据库），并配合一个配置了 `CoralSparkViewCatalog` 的本地 Spark session。测试套件覆盖：

- 视图加载，涵盖 JOIN、子查询、聚合、lateral view，以及复杂类型（数组、map、struct）
- 保留列名大小写
- 自定义 UDF 的注册与执行
- 通过 catalog 管道完成 Trino → Spark 翻译
