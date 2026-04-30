# Coral

[![CI](https://github.com/linkedin/coral/actions/workflows/ci.yml/badge.svg?branch=master)](https://github.com/linkedin/coral/actions/workflows/ci.yml)
[![release](https://img.shields.io/github/v/release/linkedin/coral?include_prereleases)](https://github.com/linkedin/coral/releases)

> 🌐 语言版本：[English](README.md) | **简体中文**

<p align="center">
 <img src="docs/coral-logo.jpg" width="400" title="Coral Logo">
</p>

**Coral** 是一款 SQL 翻译、分析与重写引擎。它构建了一个标准的中间表示 —— **Coral IR**，用于在不依赖任何具体 SQL 方言的前提下刻画关系代数表达式的语义。Coral IR 提供两种形式：一种位于抽象语法树（AST）层，另一种位于逻辑计划层。两种形式是同构的，可以相互转换。

Coral 对外暴露 API，支持 SQL 方言与 Coral IR 之间的双向转换。目前，Coral 支持将 HiveQL 和 Spark SQL 转换为 Coral IR，并支持将 Coral IR 转换为 HiveQL、Spark SQL 和 Trino SQL。借助多种 SQL 方言的支持，Coral 可以将某一种方言编写的 SQL 语句或视图定义翻译为另一方言的等价形式，也可以在不同计算引擎和基于 SQL 的数据源之间充当桥梁。方言转换的示例可参考 [coral-hive](coral-hive)、[coral-spark](coral-spark) 和 [coral-trino](coral-trino) 模块。

Coral 同时对外暴露用于 Coral IR 重写与变换的 API，包括将 Coral IR 表达式重写为语义等价但性能更优的表达式。例如，Coral 可以通过把视图定义重写为增量形式来实现增量视图维护，详见 [coral-incremental](coral-incremental) 模块。Coral 的其他重写应用还包括数据治理和策略实施。

Coral 既可以作为库集成到其他项目中，也可以作为独立服务运行。具体用法请参考下文。

## <img src="https://user-images.githubusercontent.com/10084105/141652009-eeacfab4-0e7b-4320-9379-6c3f8641fcf1.png" width="30" title="Slack Logo"> Slack

- 欢迎加入社区 Slack 一起讨论：[点击此处](https://join.slack.com/t/coral-sql/shared_invite/zt-s8te92up-qU5PSG~spK33ovPPL5v96A)！

## 模块列表

**Coral** 包含以下模块：

- **Coral-Hive**：将 HiveQL 转换为 Coral IR（通常也可用于 Spark SQL）。
- **Coral-Trino**：将 Coral IR 转换为 Trino SQL，Trino SQL 到 Coral IR 的转换仍在开发中。
- **Coral-Spark**：将 Coral IR 转换为 Spark SQL（通常也可用于 HiveQL）。
- **Coral-Dbt**：将 Coral 集成到 DBT 中，支持在 DBT 模型上应用 Coral 的变换能力。
- **Coral-Incremental**：从输入 SQL 推导出增量查询，用于增量视图维护。
- **Coral-Schema**：根据视图的逻辑计划及基础表的 Avro Schema，推导出视图的 Avro Schema。
- **Coral-Spark-Plan**（WIP）：将 Spark 计划字符串转换为等价的逻辑计划。
- **Coral-Visualization**：可视化 Coral 的 SqlNode 和 RelNode 树，并将其渲染到输出文件。
- **Coral-Service**：对外提供 REST API 的服务，便于用户与 Coral 交互（更多内容见 [Coral 即服务](#coral-即服务)）。

## 版本升级

本项目遵循语义化版本规范，版本号 `x.y.z` 分别代表主版本、次版本和补丁版本。在不同版本之间升级时，请留意以下潜在改动。

**主版本升级**

主版本升级意味着引入了不向后兼容的变更，例如删除或重命名类。

**次版本升级**

次版本升级同样会引入不向后兼容的变更，例如删除或重命名方法。

请务必仔细阅读每次版本升级配套的发布说明与迁移文档，以了解具体变化和推荐的迁移步骤。


## 如何构建

克隆仓库：

```bash
git clone https://github.com/linkedin/coral.git
```

构建：

**请注意：本项目需要 Python 3 和 Java 8 运行环境。** 请将 `JAVA_HOME` 设置为相应版本的 Java 安装目录，然后执行：

```bash
./gradlew clean build
```

或者，在命令行中通过 `org.gradle.java.home` 属性显式指定 Java 目录：

```bash
./gradlew -Dorg.gradle.java.home=/path/to/java/home clean build
```

## 贡献

项目正在积极开发中，欢迎各种形式的贡献。请先阅读 [贡献协议](CONTRIBUTING.md)。

## 参考资源

- [Coral: A SQL translation, analysis, and rewrite engine for modern data lakehouses](https://engineering.linkedin.com/blog/2020/coral)，LinkedIn 工程博客，2020/12/10。
- [Incremental View Maintenance with Coral, DBT, and Iceberg](https://www.slideshare.net/walaa_eldin_moustafa/incremental-view-maintenance-with-coral-dbt-and-iceberg)，技术分享，Iceberg Meetup，2023/05/11。
- [Coral & Transport UDFs: Building Blocks of a Postmodern Data Warehouse](https://www.slideshare.net/walaa_eldin_moustafa/coral-transport-udfs-building-blocks-of-a-postmodern-data-warehouse-229545076)，技术分享，Facebook 总部，2020/02/28。
- [Transport: Towards Logical Independence Using Translatable Portable UDFs](https://engineering.linkedin.com/blog/2018/11/using-translatable-portable-UDFs)，LinkedIn 工程博客，2018/11/14。
- [Dali Views: Functions as a Service for Big Data](https://engineering.linkedin.com/blog/2017/11/dali-views--functions-as-a-service-for-big-data)，LinkedIn 工程博客，2017/11/9。


## Coral 即服务

**Coral-as-a-Service**（简称 **Coral Service**）是一项对外暴露 REST API 的服务，用户无需借助计算引擎即可与 Coral 交互。当前服务提供两类 API：一类用于在不同 SQL 方言之间进行查询翻译；另一类用于与本地 Hive Metastore 交互，创建示例数据库、表和视图，以便在翻译 API 中引用它们。该服务支持两种运行模式：**远程 Hive Metastore 模式** 和 **本地 Hive Metastore 模式**。远程模式会复用已部署的 Hive Metastore 来解析表和视图；本地模式则创建一个空的内嵌式 Hive Metastore，供用户添加自定义的表和视图定义。

### API 参考

#### /api/translations/translate
**POST** 接口，接收包含以下字段的 JSON 请求体，返回翻译后的查询：
- `sourceLanguage`：输入方言（例如 `spark`、`trino`、`hive`，具体支持列表见下文）
- `targetLanguage`：输出方言（例如 `spark`、`trino`、`hive`，具体支持列表见下文）
- `query`：需要在两种方言之间翻译的 SQL 查询
- `rewriteType`（可选）：Coral IR 重写类型（例如 `incremental`）

#### /api/catalog-ops/execute
**POST** 接口，接收一条用于在本地 Metastore 中创建 database/table/view 的 SQL 语句
（注意：此接口仅在本地 Metastore 模式下可用）。

### 使用步骤与示例
1. 克隆 [Coral 仓库](https://github.com/linkedin/coral)
```bash
git clone https://github.com/linkedin/coral.git
```
2. 在 Coral 项目根目录下，进入 `coral-service` 模块
```bash
cd coral-service
```
3. 构建
```bash
../gradlew clean build
```
#### 使用 **本地 Metastore** 模式启动 Coral Service：
4. 运行
```bash
../gradlew bootRun --args='--spring.profiles.active=localMetastore'
```

#### 使用 **远程 Metastore** 模式启动 Coral Service：
4. 将你的 Kerberos 客户端 keytab 文件放到 `coral-service/src/main/resources`
5. 将 `coral-service/src/main/resources/hive.properties` 中所有 `SET_ME` 替换为你实际的取值
6. 运行
```
../gradlew bootRun
```
你也可以通过 `--hivePropsLocation` 指定自定义的 `hive.properties` 文件位置：
```
 ./gradlew bootRun --args='--hivePropsLocation=/tmp/hive.properties'
```
之后，你可以通过 [浏览器](#coral-service-ui) 或 [命令行](#coral-service-cli) 与服务交互。

### Coral Service UI
在 `coral-service` 模块下执行 `../gradlew bootRun --args='--spring.profiles.active=localMetastore'`（本地 Metastore 模式）或 `../gradlew bootRun`（远程 Metastore 模式）启动后端服务后，再按下列步骤配置并启动前端 UI。

注意：后端服务默认运行在 8080 端口，前端 UI 默认运行在 3000 端口。

#### 配置环境变量：
1. 在前端项目的根目录创建 `.env.local` 文件
2. 将 `.env.local.example` 的模板内容复制到新建的 `.env.local` 中
3. 在 `.env.local` 中填入实际的环境变量值

#### 在前端目录安装依赖：

```bash
npm install
```

#### 启动 Coral Service UI：
```bash
npm run dev
```
编译完成后，浏览器访问 http://localhost:3000 即可打开 UI。
<p align="center">
 <img src="docs/coral-service-ui/start.png" title="Coral Service UI">
</p>
UI 提供三类功能：

#### 本地 Metastore 模式下创建 database/table/view
该功能仅在本地 Metastore 模式下可用，底层调用上文的 `/api/catalog-ops/execute` 接口。

你可以输入一条 SQL 语句在本地 Metastore 中创建 database/table/view。
<p align="center">
 <img src="docs/coral-service-ui/creation.png" title="Coral Service Creation Feature">
</p>

#### 将 SQL 从源方言翻译为目标方言（可带重写）
该功能在本地和远程 Metastore 模式下均可用，底层调用上文的 `/api/translations/translate` 接口。

你可以输入 SQL 查询并指定源方言、目标方言，以调用 Coral 翻译服务。也可以指定重写类型，对输入查询应用相应的重写。
<p align="center">
 <img src="docs/coral-service-ui/translation.png" title="Coral Service Translation Feature">
</p>

#### 生成 Coral 中间表示的 Graphviz 可视化图
在翻译过程中，Coral 还会生成中间表示的可视化图并在界面上展示，其中也会包含重写后的节点。
<p align="center">
 <img src="docs/coral-service-ui/graphs.png" title="Coral Service Translation Feature">
</p>

#### 前端代码开发

#### 代码检查/格式化：
```bash
npm run lint:fix
npm run format
```
### Coral Service CLI
除了上面的 UI，你也可以直接通过命令行与服务交互。

以下以本地 Metastore 模式为例演示完整流程：

1. 通过 `/api/catalog-ops/execute` 接口在本地 Metastore 中创建名为 `db1` 的数据库

```bash
curl --header "Content-Type: application/json" \
  --request POST \
  --data "CREATE DATABASE IF NOT EXISTS db1" \
  http://localhost:8080/api/catalog-ops/execute

Creation successful
```
2. 通过 `/api/catalog-ops/execute` 接口在 `db1` 下创建一张名为 `airport` 的表

```bash
curl --header "Content-Type: application/json" \
  --request POST \
  --data "CREATE TABLE IF NOT EXISTS db1.airport(name string, country string, area_code int, code string, datepartition string)" \
  http://localhost:8080/api/catalog-ops/execute

Creation successful
```

3. 通过 `/api/translations/translate` 接口翻译一条针对 `db1.airport` 的查询

```bash
curl --header "Content-Type: application/json" \
  --request POST \
  --data '{
    "sourceLanguage":"hive",
    "targetLanguage":"trino",
    "query":"SELECT * FROM db1.airport"
  }' \
  http://localhost:8080/api/translations/translate
```
翻译结果为：
```
Original query in HiveQL:
SELECT * FROM db1.airport
Translated to Trino SQL:
SELECT "name", "country", "area_code", "code", "datepartition"
FROM "db1"."airport"
```


### 当前支持的翻译路径
1. Hive → Trino
2. Hive → Spark
3. Trino → Spark
   注意：Trino → Spark 翻译时，查询中引用的视图被视为以 HiveQL 定义，因此目前无法翻译定义在 Trino 中的视图。当前仅支持在 Trino 查询中引用基础表。该翻译路径目前仍处于 POC 阶段，后续仍需进一步完善。
4. Spark → Trino
