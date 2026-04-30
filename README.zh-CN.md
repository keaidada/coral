# Coral Rust

> 语言版本：[English](README.md) | **简体中文**

当前分支是 Rust 优先的 Coral 工作区。旧的 Java / Gradle 后台已经移除；当前主后端是 `coral-rust/`，Web UI 是 `coral-service/frontend/`。

## 目录结构

| 路径 | 说明 |
|---|---|
| `coral-rust/` | Rust workspace：SQL 翻译核心、HTTP 服务、CLI、FFI、schema、可视化、增量、Spark/Trino/Pig 辅助模块 |
| `coral-rust/service/` | Axum HTTP 后端，兼容 Coral Service 前端 |
| `coral-rust/cli/` | 不启动 server 的本地 SQL 翻译命令行，binary 名称为 `coral` |
| `coral-service/frontend/` | Next.js 前端，支持翻译、校验、可视化、函数库、设置与本地历史 |

## 可移植性 / 基础环境

### 只运行已编译二进制

如果使用已经构建好的 `coral` 或 `coral-service` 二进制，只需要目标系统与二进制平台匹配：

- macOS / Linux / Windows 对应平台二进制
- 不需要 JVM
- 不需要 Gradle
- 不需要数据库服务
- CLI SQL 翻译不需要启动 HTTP server

### 从源码构建 Rust 后端 / CLI

需要：

- Rust toolchain：`>= 1.75`
- Cargo
- macOS 需要 Xcode Command Line Tools：`xcode-select --install`

安装 Rust：

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup default stable
```

### 运行前端

需要：

- Node.js：建议 `20.x`
- npm
- 现代浏览器

前端只负责 UI，必须指向一个正在运行的 Rust service，默认 `http://localhost:8080`。

### 可选能力

- Python wheel / Python FFI：需要 Python 3
- C/Go/Node 等 FFI 集成：需要相应语言工具链和 `coral-rust/ffi` 产物

## CLI：不启动 server 直接转换 SQL

可以。CLI 已在 `coral-rust/cli` 中提供，crate 名是 `coral-cli`，生成的命令是 `coral`。

从源码直接运行：

```bash
cd coral-rust
echo "SELECT NVL(x, 0), y::INT FROM t" | cargo run -p coral-cli -- --source gaussdb --target spark --pretty
```

构建 release binary：

```bash
cd coral-rust
cargo build --release -p coral-cli
./target/release/coral --file query.sql --source gaussdb --target spark --pretty
```

常用参数：

```bash
# stdin 输入，默认输出 Spark SQL
echo "SELECT NVL(x, 0) FROM t" | ./target/release/coral --source gaussdb --target spark

# 也可以使用 --from / --to 别名
cat query.sql | ./target/release/coral --from hive --to trino

# Trino 输入转 Spark
cat trino.sql | ./target/release/coral --source trino --target spark --pretty

# 从文件读取
./target/release/coral --file query.sql --source gaussdb --target spark --pretty

# 默认遇到未知函数会报错；如果确认是业务 UDF，推荐指定白名单透传
echo "SELECT my_udf(x) FROM t" | ./target/release/coral --source gaussdb --target spark --allow-unknown-functions my_udf

# 多个 UDF 用逗号分隔；不带值则放行全部未知函数
echo "SELECT YNVL(x, 0), XNVL(x, 0) FROM t" | ./target/release/coral --from hive --to spark --allow-unknown-functions YNVL,XNVL

# 内置 smoke 样例
./target/release/coral --smoke

# 查看函数映射表
./target/release/coral --list-functions
```

### Shell 脚本中判断转换是否成功

`coral` 使用标准 Unix 约定：

- 退出码 `0`：转换成功，转换后的 SQL 写到 `stdout`
- 退出码非 `0`：转换失败，错误信息写到 `stderr`
- 参数错误、SQL 解析失败、未知函数未允许都会返回非 `0`

推荐脚本：

```bash
#!/usr/bin/env bash
set -euo pipefail

CORAL="./coral-linux-x86_64-musl"
INPUT_FILE="input.sql"
OUTPUT_FILE="output.sql"
ERROR_FILE="coral-error.log"

if "$CORAL" --file "$INPUT_FILE" --from gaussdb --to spark > "$OUTPUT_FILE" 2> "$ERROR_FILE"; then
  echo "SQL 转换成功: $OUTPUT_FILE"
else
  echo "SQL 转换失败" >&2
  cat "$ERROR_FILE" >&2
  exit 1
fi
```

如果需要允许业务 UDF 透传，建议指定白名单：

```bash
"$CORAL" \
  --file "$INPUT_FILE" \
  --from hive \
  --to spark \
  --allow-unknown-functions YNVL,XNVL \
  > "$OUTPUT_FILE" \
  2> "$ERROR_FILE"
```

## 后端：Rust service

```bash
cd coral-rust
cargo run -p coral-service
```

默认监听地址：`0.0.0.0:8080`。

自定义监听地址：

```bash
CORAL_BIND=127.0.0.1:9000 cargo run -p coral-service
```

健康检查：

```bash
curl http://127.0.0.1:8080/api/health
```

## 前端

```bash
cd coral-service/frontend
npm install
cp .env.local.example .env.local
npm run dev
```

在 `.env.local` 中指向 Rust 后端：

```bash
NEXT_PUBLIC_CORAL_SERVICE_API_URL=http://localhost:8080
```

浏览器打开 `http://localhost:3000`。

## 常用命令

Rust：

```bash
cd coral-rust
cargo fmt --all
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace --all-targets
cargo build --release --workspace
cargo run --release --bin coral -- --smoke
```

前端：

```bash
cd coral-service/frontend
npm run lint
npm run build
```

## HTTP API

Rust service 提供前端需要的接口：

| 方法 | 路径 | 用途 |
|---|---|---|
| `GET` | `/api/health` | 健康检查 |
| `POST` | `/api/translations/translate` | SQL 方言翻译 |
| `POST` | `/api/translations/validate` | SQL 解析 / 校验 |
| `POST` | `/api/visualizations/generategraphs` | 生成 DOT / PlantUML 图源码 |
| `GET` | `/api/visualizations/{id}` | 获取生成的图源码 |
| `GET` | `/api/functions` | 函数注册表 |
| `POST` | `/api/catalog-ops/execute` | 兼容前端 `CREATE DATABASE/TABLE/VIEW` 的接口 |

## 说明

- 不再需要 JVM 或 Gradle 后台。
- 前端历史记录保存在浏览器 `localStorage`。
- `coral-service/frontend/.next/`、`node_modules/` 和 `coral-rust/target/` 都是构建产物，不应提交。
