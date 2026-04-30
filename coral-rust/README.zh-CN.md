# coral-rust

> 🌐 语言版本：[English](README.md) | **简体中文**

[`coral-gaussdb`](../coral-gaussdb) + [`coral-gaussdb-spark`](../coral-gaussdb-spark) 的 Rust 移植版 —— GaussDB / openGauss SQL → Spark SQL 翻译器。无 JVM，单个原生二进制，单次执行约 6 毫秒。提供库、CLI 和 C FFI 绑定（Python / Go / Node 开箱即用）。

## 状态：接近生产可用的 POC

110 个测试全过。5 个阶段全部完成：

- **Stage 1** —— 全量 GaussDB 函数注册表（30 条规则，含 PG 日期格式 token 翻译）
- **Stage 2** —— 类型映射层（JSONB / UUID / BYTEA / TIMESTAMPTZ / Int2-4-8 / Float4-8 / SERIAL 家族）
- **Stage 3** —— Oracle `(+)` 外连接、窗口帧、CONNECT BY 递归
- **Stage 4** —— 可选的目录感知层，带错别字检测与 "did you mean?" 建议
- **Stage 5** —— C FFI，含 Python 与 C 示例绑定

## 为什么用 Rust

Java 版 `coral-gaussdb-spark` 基于 Apache Calcite（18 MB shaded JAR、需 JVM、冷启动 1.3 秒左右）。对于只需要 **文本 → 文本** SQL 翻译的场景（不执行查询、不查真实元数据、不做运行时类型推导），Calcite 太重。

| 指标 | Java `:coral-gaussdb-spark:smoke` | Rust `coral --smoke` |
|---|---|---|
| 冷启动（跑完 6 个样例） | 约 1266 ms | **约 6 ms**（快约 200 倍） |
| 依赖 | JVM + 15+ JAR（约 50 MB） | 单个二进制（约 2 MB） |
| 部署方式 | `java -jar ...` | `./coral` |
| 非 JVM 语言调用 | Gradle 构建 JAR + Py4J / JPype | `ctypes` / `cgo` / `ffi-napi` + `.so` |

## 架构

```
┌──────────────────────────────────────────────────────────────────┐
│ 1. 文本预处理 — preprocess/                                      │
│    Oracle START WITH / CONNECT BY     -> WITH RECURSIVE           │
│    Oracle `(+)` 外连接标记            -> LEFT JOIN ON             │
│    （两者 sqlparser-rs 都不原生支持）                             │
├──────────────────────────────────────────────────────────────────┤
│ 2. 解析 — sqlparser-rs PostgreSqlDialect                         │
│    GaussDB 与 PG 兼容，PG 方言覆盖绝大多数语法                     │
├──────────────────────────────────────────────────────────────────┤
│ 3. （可选）目录校验 — catalog::validate_against                   │
│    UnknownTable / UnknownColumn 告警，基于 Levenshtein 的         │
│    "did you mean?" 建议，CTE 感知的作用域栈                       │
├──────────────────────────────────────────────────────────────────┤
│ 4. AST 重写 pass — rewrite/                                      │
│    structure::DistinctOnRewriter  DISTINCT ON (k) -> ROW_NUMBER   │
│    functions::FunctionRewriter    30 条函数/运算符规则             │
│    types::TypeRewriter            GaussDB 类型 -> Spark 类型       │
├──────────────────────────────────────────────────────────────────┤
│ 5. 渲染 — sqlparser-rs Display                                   │
│    AST 节点通过 Display 直接输出为 Spark 可识别 SQL，               │
│    不需要单独写一个 "Spark unparser"                              │
└──────────────────────────────────────────────────────────────────┘
```

## 函数覆盖

```
聚合/窗口：      COUNT, SUM, AVG, MIN, MAX（透传）
                 ROW_NUMBER, RANK, DENSE_RANK（透传）
                 BOOL_AND/BOOL_OR        -> EVERY/SOME
                 ARRAY_AGG               -> COLLECT_LIST
                 STRING_AGG(x, sep)      -> CONCAT_WS(sep, COLLECT_LIST(x))

空值处理：       COALESCE（透传）
                 NVL                     -> COALESCE
                 NVL2(a, b, c)           -> CASE WHEN a IS NOT NULL THEN b ELSE c END
                 DECODE(x, k1, v1, ...)  -> CASE WHEN x = k1 THEN v1 ... END

字符串/正则：    SUBSTR                  -> SUBSTRING
                 POSITION(a, b)          -> INSTR(b, a)
                 REGEXP_SUBSTR(s, p, …)  -> REGEXP_EXTRACT(s, p, 0)
                 x ~ p                   -> x RLIKE p
                 x ~* p                  -> LOWER(x) RLIKE LOWER(p)
                 x !~ p, x !~* p         -> 以上的 NOT 形式

数学：           MOD(a, b)               -> a % b
                 RANDOM                  -> RAND

日期/时间：      SYSDATE, NOW            -> CURRENT_TIMESTAMP
                 TRUNC(d, 'MM')          -> DATE_TRUNC('MM', d)  （日期形式）
                 TO_CHAR(d, 'YYYY-MM')   -> DATE_FORMAT(d, 'yyyy-MM')
                 TO_DATE(s, 'YYYY-MM')   -> TO_DATE(s, 'yyyy-MM')（格式串翻译）
                 TO_TIMESTAMP            -> 同上（格式串翻译）
                 日期 token：YYYY, YY, MON, MM, MI, DD, DY, HH24/HH12/HH,
                            SS, AM/PM, FF<n> 均翻译为 Spark 对应 token

数组/序列：      GENERATE_SERIES(a, b)   -> SEQUENCE(a, b)
```

## 类型覆盖

```
JSON, JSONB, REGCLASS, TEXT              -> STRING
UUID                                     -> STRING
BYTEA                                    -> BINARY
TIMESTAMP WITH TIME ZONE / TIMESTAMPTZ   -> TIMESTAMP（去掉时区后缀）
INT2/INT4/INT8                           -> SMALLINT/INT/BIGINT
FLOAT4/FLOAT8                            -> REAL/DOUBLE
SMALLSERIAL/SERIAL/BIGSERIAL             -> SMALLINT/INT/BIGINT
INTERVAL, DATE, TIMESTAMP, DECIMAL, …    透传
```

## 结构级重写

| 输入 | 输出 |
|---|---|
| `SELECT DISTINCT ON (k) ... ORDER BY ...` | `SELECT ... FROM (SELECT ..., ROW_NUMBER() OVER (...) AS rn) WHERE rn = 1` |
| `SELECT ... START WITH ... CONNECT BY PRIOR id = pid` | `WITH RECURSIVE __coral_connect_by AS (...) SELECT ...` |
| `FROM a, b WHERE a.id = b.id(+)` | `FROM a LEFT JOIN b ON a.id = b.id` |

## 用法

### CLI

```bash
cargo build --release -p coral-cli

# 从 stdin 读：
echo "SELECT NVL(x, 0), y::INT FROM t" | ./target/release/coral
# → SELECT COALESCE(x, 0), CAST(y AS INT) FROM t

# 从文件读：
./target/release/coral --file query.sql

# 内置演示（与 :coral-gaussdb-spark:smoke 同样的 6 个样例）：
./target/release/coral --smoke
```

### 库调用

```toml
[dependencies]
coral-core = { path = "path/to/coral-rust/core" }
```

```rust
// 普通翻译
let spark_sql = coral_core::translate("SELECT DECODE(x, 1, 'a', 'b') FROM t")?;
// "SELECT CASE WHEN x = 1 THEN 'a' ELSE 'b' END FROM t"

// 带目录校验
use coral_core::{InMemoryCatalog, translate_with_catalog};

let cat = InMemoryCatalog::from_pairs(&[
    ("default", "employees", &["id|int", "name|string", "dept_id|int"]),
]);
let r = translate_with_catalog("SELECT e.dpt_id FROM default.employees e", &cat)?;
// r.issues[0] -> UnknownColumn { column: "dpt_id", did_you_mean: Some("dept_id") }
// r.spark_sql -> 翻译后的 SQL（无论是否有告警都返回）
```

### Python

```bash
cargo build --release -p coral-ffi
python3 ffi/examples/python/smoke_demo.py
```

```python
from coral import translate, CoralError

try:
    spark_sql = translate("SELECT NVL(x, 0), y::INT FROM t")
    print(spark_sql)
except CoralError as e:
    print("translation failed:", e)
```

不依赖 PyO3。无需构建步骤。用标准库的 `ctypes` 直接加载 `libcoral_ffi.dylib/.so/.dll` 即可。

### C / C++

```bash
cargo build --release -p coral-ffi

cc ffi/examples/c/smoke.c -o smoke \
   -I ffi/include -L target/release -lcoral_ffi \
   -Wl,-rpath,'$ORIGIN/target/release'
./smoke
```

头文件在 `ffi/include/coral.h`，总共只有 4 个函数：
```c
char       *coral_translate(const char *input);    // 返回拥有所有权的指针，失败返回 NULL
void        coral_free_string(char *ptr);
const char *coral_last_error(void);                // 线程局部，借用
const char *coral_version(void);                   // static 生命周期，借用
```

## 构建与测试

```bash
cd coral-rust
cargo build --release
cargo test                       # 110 个测试，跨 4 个 crate
cargo run --bin coral -- --smoke
```

## 暂未覆盖

本 POC 只解决翻译问题，**不**替代 Coral 的以下能力：

- Hive Metastore 远程目录（trait 已提供，随本项目只附带 in-memory 实现）
- 运行时 UDF 注册表与 Hive 语义（StaticHiveFunctionRegistry 的 100+ 条规则）
- `coral-hive` / `coral-trino` / `coral-spark` / `coral-incremental` / `coral-schema`

如果以上能力你需要，请继续使用 Java 版。如果只是想在 Rust / Python / Go / Node / C 里把 GaussDB SQL 字符串翻译成 Spark SQL 字符串，这就是合适的工具。

## 许可证

BSD-2-Clause，与 Coral 主项目一致。
