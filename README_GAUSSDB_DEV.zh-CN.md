# Coral coral-gaussdb 开发：文档索引

> 🌐 语言版本：[English](README_GAUSSDB_DEV.md) | **简体中文**

## 📚 三份核心文档

本次调研共输出了 **三份系统性指南**，帮助你从零构建 `coral-gaussdb` 模块：

### 1. **QUICK_REFERENCE.md** ⭐ 从这里开始
   - **适用场景**：快速定位、调试
   - **内容**：
     - 直接可点击的文件路径引用
     - 关键入口及对应行号
     - 需要重写的 5 个抽象方法
     - GaussDB 与 Hive 差异对照表
     - 开发路线图（4 个阶段，约 1-2 周）
   - **篇幅**：约 4 页
   - **何时使用**：需要查某个具体类、方法，或想快速获得答案时

---

### 2. **CORAL_ARCHITECTURE_ANALYSIS.md** 📖 详细参考手册
   - **适用场景**：理解 Coral 的整体设计
   - **内容**：
     - 完整模块布局（coral-common、coral-hive、coral-trino、coral-spark）
     - 关键基类与接口（5 个核心抽象）
     - 入口点与 API 形态（使用者如何调用库）
     - 函数映射模式（Hive `substr` → Spark `substring`）
     - Gradle 构建拓扑（模块之间的依赖关系）
     - 测试模式（以 HiveToRelConverterTest 为例）
     - Apache Calcite 集成细节（v1.21.0.265 分支）
   - **篇幅**：约 8 页
   - **何时使用**：需要深入理解 Coral 的运作机制时

---

### 3. **CORAL_GAUSSDB_TEMPLATE.md** 🏗️ 实现指南
   - **适用场景**：真正动手开发 coral-gaussdb
   - **内容**：
     - 架构图（GaussDB SQL → RelNode → Spark SQL）
     - 目录结构（需要创建哪些文件）
     - 分阶段实施计划（4 个阶段）
     - 代码模板（GaussdbToRelConverter 骨架等）
     - GaussDB 专属注意事项（字符串拼接 `||`、数组、JSON）
     - 兼容性策略（复用 vs 新建）
     - 测试样板示例
     - 完整的检查清单
   - **篇幅**：约 10 页
   - **何时使用**：准备开始写代码时

---

## 🚀 入门指引（5 分钟路线）

1. **阅读 QUICK_REFERENCE.md**（5 分钟）
   - 了解必须重写的 5 个方法
   - 明确需要精读的文件路径

2. **精读 HiveToRelConverter**（15 分钟）
   - 打开：`/coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/HiveToRelConverter.java`
   - 阅读第 48-133 行（展示了 GaussdbToRelConverter 的实现模式）

3. **浏览 CORAL_ARCHITECTURE_ANALYSIS.md**（10 分钟）
   - 重点关注第 1-3 节（模块布局、基类、入口点）

4. **打开 CORAL_GAUSSDB_TEMPLATE.md**（10 分钟）
   - 阅读 "What Makes GaussDB Different from Hive"（GaussDB 与 Hive 的差异）
   - 查看 Phase 1（核心搭建）

5. **开始编码**
   - 创建 `coral-gaussdb/` 目录
   - 按 CORAL_GAUSSDB_TEMPLATE.md 中的 Phase 1 一步步做

---

## 📊 信息结构一览

```
QUICK_REFERENCE.md
├─ 精读入口（6 个关键类 + 行号）
├─ 文件路径速查表（便于复制）
├─ Gradle 配置
├─ Calcite 关键类（解释 8 个类）
├─ GaussDB vs Hive（对照表）
├─ 测试快速入门
├─ 必须重写的关键方法
├─ 开发路线图
└─ 调试技巧

CORAL_ARCHITECTURE_ANALYSIS.md
├─ 模块布局（coral-common、coral-hive、coral-trino、coral-spark）
├─ 关键类（入口、解析器、函数映射）
├─ 构建配置（Gradle 写法）
├─ 测试模式（字符串进，字符串出）
├─ Apache Calcite 集成
└─ 可直接套用的 coral-gaussdb 模板

CORAL_GAUSSDB_TEMPLATE.md
├─ 架构图
├─ 目录结构（需要创建的精确文件清单）
├─ Phase 1：模块脚手架
├─ Phase 2：关键类（含代码骨架）
├─ Phase 3：GaussDB 专项处理
├─ Phase 4：实现顺序
├─ Phase 5：测试模式
└─ 14 项完成检查清单
```

---

## 🎯 开发检查清单（摘自 CORAL_GAUSSDB_TEMPLATE.md）

### Phase 1：创建核心模块文件（1 天）
- [ ] 在 `settings.gradle` 中加入 `include 'coral-gaussdb'`
- [ ] 编写 `coral-gaussdb/build.gradle` 及其依赖
- [ ] 创建目录 `coral-gaussdb/src/main/java/.../gaussdb2rel/`

### Phase 2：实现解析器与校验器（2-3 天）
- [ ] `GaussdbSqlConformance.java` —— SQL 语法规则
- [ ] `GaussdbSqlValidator.java` —— 继承自 HiveSqlValidator
- [ ] `GaussdbSqlToRelConverter.java` —— Calcite 桥接层
- [ ] 测试基础的 SELECT、WHERE、FROM 解析

### Phase 3：函数注册表（2-3 天）
- [ ] `StaticGaussdbFunctionRegistry.java` —— 200+ 个函数
- [ ] 映射字符串拼接运算符 `||`
- [ ] 映射数组相关函数（PostgreSQL 风格的数组）
- [ ] 映射 JSON 相关函数

### Phase 4：集成与测试（1-2 天）
- [ ] `GaussdbToRelConverterTest.java` —— 覆盖面足够的测试
- [ ] 端到端测试：GaussDB SQL → RelNode → Spark SQL
- [ ] 性能测试

---

## 🔗 直接文件引用

### 必读（按顺序）
1. **HiveToRelConverter** → `coral-hive/src/main/java/.../hive/hive2rel/HiveToRelConverter.java`
   - 编写 GaussdbToRelConverter 的模板
2. **ToRelConverter** → `coral-common/src/main/java/.../common/ToRelConverter.java`
   - 抽象基类（理解其中的 5 个方法）
3. **StaticHiveFunctionRegistry** → `coral-hive/src/main/java/.../hive/hive2rel/functions/StaticHiveFunctionRegistry.java`
   - 编写 StaticGaussdbFunctionRegistry 的模板
4. **HiveToRelConverterTest** → `coral-hive/src/test/java/.../hive/hive2rel/HiveToRelConverterTest.java`
   - 测试用例的模板

### 参考
- **CoralSpark** → `coral-spark/src/main/java/.../spark/CoralSpark.java`
  - 展示后端如何消费 RelNode IR
- **FunctionRegistry** → `coral-common/src/main/java/.../common/functions/FunctionRegistry.java`
  - 你需要实现的接口

---

## 💡 关键洞察

### coral-gaussdb 要做什么
```
GaussDB SQL + PostgreSQL 函数 + 数组 + JSON
        ↓
     使用 Calcite 解析
        ↓
GaussDB 函数 → Calcite SqlOperator
        ↓
   生成 RelNode IR（与方言无关）
        ↓
   喂给 coral-spark 或其它后端
        ↓
     Spark SQL / 其它输出
```

### GaussDB 的专项挑战
1. **字符串拼接** —— 使用 `||` 运算符（而不是 `concat()`）
2. **数组语法** —— 需解析 `ARRAY[1,2,3]`（而不是 `ARRAY(1,2,3)`）
3. **JSON 支持** —— JSONB 是一等类型（在 IR 中映射为 STRING）
4. **PostgreSQL 函数** —— 约 200+ 个 Hive 不具备的函数（array_agg、string_agg、date_trunc 等）
5. **类型系统** —— 将 PostgreSQL 类型映射到 Coral 类型

### 复用策略
- **ParseTreeBuilder** —— 可复用 Hive 的解析器（GaussDB/PostgreSQL 兼容）
- **CoralConvertletTable** —— 复用 Hive 的实现
- **DaliOperatorTable** —— 扩展加入 GaussDB 相关算子
- **函数注册表** —— 新建，但参考 Hive 的范式
- **RelBuilder** —— 复用 HiveRelBuilder

---

## ❓ 常见问题

**Q：我需要自己写解析器吗？**
A：不用！GaussDB 与 PostgreSQL 兼容，你可以复用 Hive 的 ANTLR 解析器，或者直接借用 Trino 的解析器（已内置 PostgreSQL 语法支持）。

**Q：我需要映射多少个函数？**
A：先把约 50 个常用函数跑通，然后再扩展。StaticHiveFunctionRegistry 目前有 100+ 个。Coral 对未知函数支持优雅降级，可安全跳过。

**Q：coral-gaussdb 能配合 coral-spark 一起工作吗？**
A：可以！RelNode IR 与具体方言无关。只要你能把 GaussDB SQL 转成 RelNode，coral-spark 就能自动把它转成 Spark SQL。

**Q：如果 GaussDB 里有 Spark 不支持的函数怎么办？**
A：Calcite 会以 "未识别函数调用" 的形式保留它。当 coral-spark 试图生成 Spark SQL 时，会：
   1. 映射到等价的 Spark 函数
   2. 抛错（如果确实不受支持）
   3. 原样透传（如果 Spark 能解析）

**Q：需要多长时间？**
A：实现一个可用的模块大约需要 1-2 周。先覆盖约 100 个函数，打通端到端链路，再做扩展。

---

## 📞 支持资源

### 遇到问题时
1. **查看 QUICK_REFERENCE.md** 获取调试技巧
2. **参考 HiveToRelConverter** —— 它就是你的模板
3. **研究 StaticHiveFunctionRegistry** —— 掌握函数注册的通用模式
4. **阅读测试** —— HiveToRelConverterTest 展示了期望的行为模式
5. **Calcite 文档** —— 关于 SqlOperator、SqlConformance、SqlValidator 的官方说明

### 外部参考
- **GaussDB / openGauss 文档** —— 函数参考与类型系统
- **PostgreSQL 文档** —— GaussDB 与 PostgreSQL 兼容
- **Calcite 文档** —— SqlNode、SqlOperator、RelNode 相关
- **Coral README** —— 项目整体介绍

---

## 📈 成功指标

当以下条件满足时，说明你的实现已经可用：
1. ✅ `gradle build coral-gaussdb` 能成功构建
2. ✅ 基础的 SELECT 解析能通过
3. ✅ 函数解析可用（例如 `substr` 能对应到 Calcite 算子）
4. ✅ GaussDB SQL → RelNode 转换能跑通
5. ✅ RelNode → Spark SQL 端到端可用
6. ✅ 测试套件通过（仿照 HiveToRelConverterTest 的风格）

---

## 🎓 学习路径

**第 1 天：理解**
- 阅读 QUICK_REFERENCE.md（30 分钟）
- 精读 HiveToRelConverter（1 小时）
- 浏览 CORAL_ARCHITECTURE_ANALYSIS.md（30 分钟）

**第 2 天：规划**
- 阅读 CORAL_GAUSSDB_TEMPLATE.md（1 小时）
- 将 GaussDB 函数映射到 Calcite 算子（表格化，约 2 小时）
- 规划目录结构

**第 3-5 天：实现**
- Phase 1（核心模块搭建）—— 1 天
- Phase 2（解析与校验）—— 2 天
- Phase 3（函数注册表）—— 2 天
- Phase 4（测试）—— 1 天

---

## 📝 说明

- 这些文档基于 2026/04/28 对 Coral 代码库的分析生成
- 所有行号与文件路径以该日期为准
- Coral 版本：LinkedIn 基于 Calcite 1.21.0.265 的定制分支
- Hive 版本：1.2.2
- 支持的 Spark 版本：2.4.0、3.1.1、3.5.0

---

**建议先读 QUICK_REFERENCE.md，等准备好写代码时再切到 CORAL_GAUSSDB_TEMPLATE.md！**
