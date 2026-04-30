# Coral SQL Translation Architecture Report

## Overview
Coral is a LinkedIn SQL translation framework built on **Apache Calcite v1.21.0.265** (LinkedIn fork). It converts SQL from different dialects to a unified relational algebra intermediate representation (IR) via Calcite's `RelNode`, then translates to target SQL dialects.

---

## 1. Module Layout & Key Classes

### **coral-common** (Shared Abstractions)
**Location:** `/coral-common/src/main/java/com/linkedin/coral/common/`

**Core Classes:**
- `ToRelConverter.java` — Abstract base class for all SQL-to-RelNode converters
  - Provides framework configuration, Calcite schema setup
  - Subclasses override: `getConvertletTable()`, `getSqlValidator()`, `getOperatorTable()`, `getSqlToRelConverter()`
- `HiveRelBuilder.java` — Custom RelBuilder for Hive-compatible relational operations
- `HiveTypeSystem.java` — Custom type system (Hive semantics)
- `TypeConverter.java`, `HiveToCoralTypeConverter.java` — SQL type ↔ Coral type mapping
- `functions/FunctionRegistry.java` — Interface for function resolution
- `functions/Function.java` — Represents a SQL function with return type & operand rules

**Catalogs:**
- `CoralCatalog`, `HiveTable`, `IcebergTable` — Multi-format table abstraction
- `CoralRootSchema.java` — Calcite schema provider

---

### **coral-hive** (Frontend: Hive SQL → RelNode)
**Location:** `/coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/`

**Key Classes:**
1. **Entry Point:** `HiveToRelConverter.java` (extends `ToRelConverter`)
   - Public API: `convertSql(String sql)` → `RelNode`
   - Manages ParseTreeBuilder, Function Resolver, SQL Validator

2. **Parser:** `parsetree/ParseTreeBuilder.java`
   - Uses ANTLR grammar (`src/main/antlr/roots/`) to build Calcite `SqlNode` AST
   - Grammar files: Hive SQL syntax rules

3. **SQL Processing:**
   - `HiveSqlToRelConverter.java` — Bridges Calcite's `SqlToRelConverter` 
   - `HiveSqlValidator.java` — Validates & semantically checks Hive SQL
   - `HiveSqlNodeToCoralSqlNodeConverter.java` — Normalizes SqlNode (Coral conventions)

4. **Function Mapping:**
   - `functions/StaticHiveFunctionRegistry.java` — Hard-coded registry of ~100+ Hive functions
   - Maps Hive function names → Calcite `SqlOperator` objects with type inference rules
   - Example: `substr(str, pos, len)` maps to Calcite's `SUBSTRING` operator

5. **Operator Customization:**
   - `DaliOperatorTable.java` — ChainedOperatorTable combining Calcite standard + Hive custom operators
   - `CoralConvertletTable.java` — SQL expression → Calcite RexNode conversion rules

6. **Build Config:** `build.gradle`
   ```gradle
   apply plugin: 'antlr'
   dependencies {
     antlr deps.'antlr'
     api project(':coral-common')
     implementation deps.'antlr-runtime'
     testImplementation deps.'hive'.'hive-exec-core'
   }
   generateGrammarSource { arguments += ['-lib', 'src/main/antlr/imports'] }
   ```

---

### **coral-trino** (Frontend: Trino SQL → RelNode)
**Location:** `/coral-trino/src/main/java/com/linkedin/coral/trino/`

**Key Classes:**
1. **Entry Point:** `trino2rel/TrinoToRelConverter.java` (extends `ToRelConverter`)
   - Same interface as HiveToRelConverter but parses Trino SQL syntax

2. **Parser:** `trino2rel/parsetree/ParseTreeBuilder.java`
   - Uses shaded Trino parser (dependency: `:shading:coral-trino-parser`)
   - Builds Calcite `SqlNode` from Trino SQL text

3. **Operator Mapping:**
   - Reuses `DaliOperatorTable` and `StaticHiveFunctionRegistry` from coral-hive
   - Trino functions normalized to Hive equivalents → Calcite operators

4. **Build Config:**
   ```gradle
   dependencies {
     implementation project(':coral-hive')
     implementation project(':shading:coral-trino-parser', configuration: 'shadow')
   }
   ```

---

### **coral-spark** (Backend: RelNode → Spark SQL)
**Location:** `/coral-spark/src/main/java/com/linkedin/coral/spark/`

**Key Classes:**
1. **Entry Point:** `CoralSpark.java`
   - Public API: `CoralSpark.create(RelNode irRelNode, HmsClient)` → CoralSparkInfo
   - Returns: Spark SQL string + base tables + UDF info

2. **IR → Spark Rel Transformation:**
   - `IRRelToSparkRelTransformer.java` — Converts Calcite RelNode (IR) → Spark-specific RelNode
     - Adds Spark-specific optimizations, enforces Spark operator semantics
   - Returns `SparkRelInfo` containing RelNode + detected UDFs

3. **Spark Rel → Spark SQL:**
   - `SparkRelToSparkSqlConverter.java` (via `CoralRelToSqlNodeConverter` from coral-common)
   - Uses Calcite's `SqlDialect` for SQL generation

4. **SQL Output:**
   - `dialect/SparkSqlDialect.java` — Spark-specific SQL dialect
     - Converts `ARRAY[]` → `ARRAY()`
     - Converts `UNNEST` → `EXPLODE`
     - Quote style: backticks (`` ` ``) for identifiers

5. **Function Mapping (Rel → Spark SQL):**
   - `CoralToSparkSqlCallConverter.java` — Converts Calcite function calls
   - Uses `SparkSqlRewriter.java` for post-generation rewrites
   - Example: `SUBSTRING(...)` rewritten to Spark semantics

6. **Build Config:**
   ```gradle
   dependencies {
     implementation project(':coral-hive')
     implementation project(':coral-schema')
     compileOnly deps.'spark'.'sql'
   }
   ```

---

## 2. Key Base Classes & Interfaces (coral-common)

**All new dialects must implement:**

| Class | Purpose | Methods to Override |
|-------|---------|---------------------|
| `ToRelConverter` | Abstract base for SQL→RelNode | `getConvertletTable()`, `getSqlValidator()`, `getOperatorTable()`, `getSqlToRelConverter()`, `toSqlNode()` |
| `FunctionRegistry` | Function name → SqlOperator mapping | `lookup(String name)`, `getAll()` |
| `TypeConverter` | SQL type ↔ Coral type | `convert(HiveType)` |
| `SqlValidator` | (from Calcite) validates SQL | Usually reuse Hive's `HiveSqlValidator` or subclass |

---

## 3. Entry Points & API Shape

### Convert Hive SQL to RelNode:
```java
HiveToRelConverter converter = new HiveToRelConverter(catalog);
RelNode irRel = converter.convertSql("SELECT * FROM table");
```

### Convert Trino SQL to RelNode:
```java
TrinoToRelConverter converter = new TrinoToRelConverter(catalog);
RelNode irRel = converter.convertSql("SELECT * FROM table");
```

### Convert RelNode to Spark SQL:
```java
CoralSpark spark = CoralSpark.create(irRel, hmsClient);
String sparkSql = spark.getSparkSql();
```

---

## 4. Function Mapping Pattern

**Example: Hive `substr` → Spark `substring`**

1. **Hive→IR** (in `StaticHiveFunctionRegistry`):
   ```java
   registerBuiltInFunction("substr", SqlStdOperatorTable.SUBSTRING, 
       family(SqlTypeFamily.STRING), family(SqlTypeFamily.INTEGER), ...);
   ```

2. **Registry** maps `"substr"` → Calcite's `SUBSTRING` operator (SqlOperator)

3. **IR→Spark** (in `CoralToSparkSqlCallConverter`):
   - Calcite `SUBSTRING` call is visited
   - May apply Spark-specific rewrites (e.g., argument reordering)
   - Emits `substring(...)` in Spark SQL

**Pattern for adding new function:**
1. Add entry to `StaticHiveFunctionRegistry.java` (or new registry class)
2. Define return type & operand type rules using Calcite's `ReturnTypes` and `OperandTypes`
3. Add SqlNode→SparkSql conversion in `CoralToSparkSqlCallConverter` if Spark semantics differ

---

## 5. Gradle Wiring

### settings.gradle (Module Registration):
```gradle
include 'coral-common'
include 'coral-hive'
include 'coral-trino'
include 'coral-spark'
```

### Module Dependencies:
- **coral-hive**: depends on `coral-common` + ANTLR
- **coral-trino**: depends on `coral-hive` + shaded Trino parser
- **coral-spark**: depends on `coral-hive` + `coral-schema`
- **coral-common**: no dialect dependencies (pure abstractions)

### Build Plugins:
- `coral-hive`: uses `antlr` plugin (generates parser from grammar)
- `coral-trino`: no ANTLR (uses pre-built Trino parser jar)

---

## 6. Testing Pattern

**Test Structure:** String-in, RelNode-out or String-in, String-out

### HiveToRelConverterTest.java:
```java
@BeforeClass
public static void beforeClass() throws IOException, HiveException {
  conf = TestUtils.loadResourceHiveConf();
  ToRelConverterTestUtils.setup(conf);
}

@Test
public void testSubstringFunction() {
  HiveToRelConverter converter = new HiveToRelConverter(catalog);
  RelNode relNode = converter.convertSql("SELECT substr('hello', 1, 2)");
  // assertions on RelNode structure
}
```

### CoralSparkTest.java:
```java
@Test
public void testSparkSqlGeneration() {
  RelNode irRel = new HiveToRelConverter(catalog).convertSql("SELECT ...");
  CoralSpark spark = CoralSpark.create(irRel, hmsClient);
  String sql = spark.getSparkSql();
  assertEquals("SELECT ... ", sql);
}
```

**Test Resources:**
- Located in `src/test/resources/` (Hive metastore configs, test DDLs)
- Uses in-memory or file-based Hive metastore for table metadata

---

## 7. Apache Calcite Integration

**Yes, Coral is built entirely on Calcite.**

- **Version:** `com.linkedin.calcite:calcite-core:1.21.0.265` (LinkedIn fork of Calcite 1.21)
- **Key Calcite Components Used:**
  - `org.apache.calcite.sql.SqlNode` — SQL AST
  - `org.apache.calcite.sql.SqlOperator` — Function/operator definitions
  - `org.apache.calcite.rel.RelNode` — Relational algebra IR
  - `org.apache.calcite.sql.validate.SqlValidator` — Type checking
  - `org.apache.calcite.sql2rel.SqlToRelConverter` — SQL→RelNode conversion
  - `org.apache.calcite.sql.SqlDialect` — SQL generation (unparse)

**Each dialect's Calcite dialect:**
- **Hive**: Uses Calcite's default `HIVE` SqlDialect (indirectly)
- **Trino**: Normalizes to Hive semantics for IR generation
- **Spark**: Uses custom `SparkSqlDialect` (in `dialect/SparkSqlDialect.java`)
  - Extends Calcite's `SqlDialect` base class
  - Overrides `unparseCall()` for Spark-specific syntax (EXPLODE, ARRAY(), etc.)

---

## Template for coral-gaussdb Module

Based on analysis, a new `coral-gaussdb` frontend would need:

**Package:** `com.linkedin.coral.gaussdb.gaussdb2rel`

**Core Classes (minimal):**
1. `GaussdbToRelConverter.java` (extends `ToRelConverter`)
2. `GaussdbSqlToRelConverter.java` (wraps Calcite's `SqlToRelConverter`)
3. `GaussdbSqlValidator.java` (extends or reuses `HiveSqlValidator`)
4. `GaussdbSqlConformance.java` (defines SQL syntax rules for Calcite parser)
5. `functions/GaussdbFunctionRegistry.java` (maps GaussDB functions to Calcite operators)

**Build file:** `build.gradle` (similar to coral-hive if using ANTLR, or coral-trino if using existing parser)

**Tests:** Mirror `HiveToRelConverterTest` pattern in `GaussdbToRelConverterTest.java`

