# coral-rust

> 🌐 语言版本：[English](README.md) | **简体中文**

[`coral-gaussdb`](../coral-gaussdb) + [`coral-gaussdb-spark`](../coral-gaussdb-spark) 的 Rust 移植版 —— GaussDB / openGauss SQL → Spark SQL 翻译器。无 JVM，单个原生二进制，单次执行约 5 毫秒。

## 状态：POC ✅

概念验证版本，覆盖了 Java 版 `SmokeDemo` 里同样的 6 个代表性样例。15 个测试全绿。这**不是**对 Coral 的完整移植 —— 见下文 **范围** 一节。

## 为什么用 Rust

Java 版 `coral-gaussdb-spark` 基于 Apache Calcite 构建（18 MB shaded JAR、需要 JVM、冷启动 1 秒以上）。对于只需要 **文本 → 文本** SQL 翻译的场景（不执行查询、不查元数据目录、不做基于真实 schema 的类型推导），Calcite 太重了。

| 指标 | Java `:coral-gaussdb-spark:smoke` | Rust `coral --smoke` |
|---|---|---|
| 冷启动（跑完 6 个样例） | 约 1266 ms | **约 6 ms** |
| 依赖 | JVM + 15+ JAR（约 50 MB） | 单个二进制（约 2 MB） |
| 部署方式 | `java -jar ...` | `./coral` |

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│ 1. 文本预处理（preprocess.rs）                                │
│    把 Oracle 风格的 START WITH / CONNECT BY 重写为标准的       │
│    WITH RECURSIVE —— sqlparser-rs 不直接支持 Oracle 层级查询。  │
├─────────────────────────────────────────────────────────────┤
│ 2. 解析（sqlparser-rs 的 PostgreSqlDialect）                  │
│    GaussDB 与 PostgreSQL 兼容，因此对 coral-gaussdb 处理的语法   │
│    结构都能正确解析。                                         │
├─────────────────────────────────────────────────────────────┤
│ 3. AST 重写 pass（rewrite/）                                  │
│    - structure::DistinctOnRewriter                          │
│        DISTINCT ON (k) -> ROW_NUMBER() OVER 子查询            │
│    - functions::FunctionRewriter                            │
│        NVL / NVL2 / DECODE / SUBSTR / MOD / SYSDATE / RANDOM │
│        以及 PG 正则 ~ ~* !~ !~* 和 :: 类型转换                 │
├─────────────────────────────────────────────────────────────┤
│ 4. 渲染（sqlparser-rs 的 Display trait）                      │
│    每个 AST 节点通过 Display 输出成 Spark 可识别的 SQL，无需      │
│    额外写一个 "Spark unparser"。                             │
└─────────────────────────────────────────────────────────────┘
```

## 与 Coral Java 的覆盖度对比

| 样例（来自 SmokeDemo） | Java coral-gaussdb-spark | coral-rust |
|---|---|---|
| CTE + JOIN + 窗口 + NVL + `\|\|` | ✅ | ✅ `NVL → COALESCE` |
| `::INT` / DECODE / SUBSTR / MOD | ✅ | ✅ 4 项全部重写 |
| `~*` / `~` 正则运算符 | ✅ | ✅ `LOWER(x) RLIKE LOWER(p)` / `x RLIKE p` |
| MERGE INTO | ✅ | ✅ 透传（语法兼容） |
| CONNECT BY 递归 | ✅ | ✅ 文本级预处理 → WITH RECURSIVE |
| DISTINCT ON (k) | ✅ | ✅ ROW_NUMBER() 子查询 |

## 范围

**本 POC 只做翻译，不做执行。** 它**不**替代 Coral 的以下能力：

- 目录解析（查 Hive Metastore 读 schema）—— coral-rust 是纯文本到文本
- 跨调用的类型推导（Calcite 丰富的类型系统）
- UDF 注册表（StaticHiveFunctionRegistry 的 100+ 函数映射）
- `coral-hive`、`coral-trino`、`coral-spark`、`coral-incremental`、`coral-schema` 等模块

如果你需要以上能力中的任何一项，请使用 Java 版。如果你只想在一个原生二进制里把 GaussDB SQL 字符串翻译成 Spark SQL 字符串，这就是合适的工具。

## 用法

### CLI

```bash
# 从 stdin 读：
echo "SELECT NVL(x, 0), y::INT FROM t" | coral
# → SELECT COALESCE(x, 0), CAST(y AS INT) FROM t

# 从文件读：
coral --file query.sql

# 内置演示（与 :coral-gaussdb-spark:smoke 同样的 6 个样例）：
coral --smoke
```

### 库调用

```toml
[dependencies]
coral-core = { path = "path/to/coral-rust/core" }
```

```rust
let spark_sql = coral_core::translate("SELECT DECODE(x, 1, 'a', 'b') FROM t")?;
// "SELECT CASE WHEN x = 1 THEN 'a' ELSE 'b' END FROM t"
```

## 构建与测试

```bash
cd coral-rust
cargo build --release        # target/release/coral
cargo test                   # 15 个测试：3 个单元测试 + 12 个集成测试
cargo run --bin coral -- --smoke
```

## 扩展

新增一个函数映射，在 `core/src/rewrite/functions.rs` 里约 5 行代码搞定：

```rust
"to_char" => {
    // 把 TO_CHAR(x, 'YYYY') 转为 DATE_FORMAT(x, 'yyyy')（含格式串翻译）
    // ...
}
```

新增一个结构级重写，以 `DistinctOnRewriter` 为模板：实现 `VisitorMut`，覆写 `post_visit_query`，重连 `Query` 节点。

对于 sqlparser-rs 无法解析的语法（例如 CONNECT BY），扩展 `core/src/preprocess.rs` 做文本级变换。

## 限制

- **MERGE INTO** 透传前提是目标表支持 Spark MERGE（Delta / Iceberg / Hudi）。普通 Hive 表会在执行阶段报错 —— 这是 Spark 的限制，不是翻译问题。
- **窗口帧**（`ROWS BETWEEN ... AND ...`）可以解析但未覆盖测试；依赖 `Display` 回写，可能与 Coral Java 版的输出存在细微差异。
- **CONNECT BY 预处理器** 仅处理规范形状（FROM 里是裸表、PRIOR 是简单等值）。复杂情况会直接落到 parser 报语法错，不会悄悄产出错误 SQL。
- **类型系统**：GaussDB 的 `JSONB`、`UUID`、`INTERVAL` 会原样传到输出；Spark 如果不识别会抛错。类型映射层属于后续工作。

## 许可证

BSD-2-Clause —— 与 Coral 主项目一致。
