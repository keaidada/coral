# Coral Project Structure Analysis: Guide for coral-gaussdb Module

## Executive Summary

Coral is a **SQL dialect translation framework** built on **Apache Calcite** that converts SQL from different sources into intermediate representation (IR), then to target dialects. The pattern is: **Frontend (SQL → RelNode) → IR (RelNode) → Backend (RelNode → Target SQL)**.

---

## 1. MODULE LAYOUT & ARCHITECTURE

### Three Key Roles:
1. **Frontends** (SQL → RelNode): `coral-hive`, `coral-trino` — parse dialect SQL into Calcite's RelNode IR
2. **IR Core**: `coral-common` — shared abstractions, type system, catalog
3. **Backends** (RelNode → SQL): `coral-spark`, `coral-trino` (also converts back to Trino) — generate target SQL

### coral-hive (Frontend Template)
**Path**: `/Users/liwei/work/study/bigdata/coral/coral-hive`

**Key Classes**:
- `HiveToRelConverter` — **PUBLIC ENTRY POINT** for Hive SQL → RelNode conversion
- `HiveSqlToRelConverter` — Internal Calcite SqlToRelConverter wrapper
- `HiveSqlValidator` — Validates Hive SQL using Calcite's SqlValidator
- `HiveViewExpander` — Expands view definitions
- `ParseTreeBuilder` — Builds AST from ANTLR grammar (Hive uses Hive's own parser)
- **Packages**:
  - `hive2rel/` — Main conversion logic
  - `hive2rel/functions/` — Function mappings (HiveFunctionResolver, StaticHiveFunctionRegistry)
  - `hive2rel/parsetree/` — ANTLR parse tree handling

**Gradle** (`coral-hive/build.gradle`):
- Applies `antlr` plugin (generates parser from `.g4` grammar)
- Depends on `coral-common`
- ANTLR sources in `src/main/antlr/`

### coral-trino (Frontend Template with Custom Parser)
**Path**: `/Users/liwei/work/study/bigdata/coral/coral-trino`

**Key Classes**:
- `TrinoToRelConverter` — **PUBLIC ENTRY POINT** for Trino SQL → RelNode conversion
- `TrinoSqlToRelConverter` — Internal Calcite SqlToRelConverter wrapper
- `TrinoSqlValidator`, `TrinoSqlConformance` — Trino-specific SQL validation
- **Packages**:
  - `trino2rel/parsetree/` — Custom Trino parser (TrinoParserDriver, ParseTreeBuilder)
  - `rel2trino/` — **BIDIRECTIONAL**: also converts RelNode → Trino SQL
  - `rel2trino/TrinoSqlDialect` — Calcite SqlDialect for Trino output
  - `rel2trino/RelToTrinoConverter` — RelNode → Trino SQL converter

**Gradle** (`coral-trino/build.gradle`):
- Depends on `shading:coral-trino-parser` (shaded Trino parser jar)
- Implements both directions of translation

### coral-spark (Backend Template)
**Path**: `/Users/liwei/work/study/bigdata/coral/coral-spark`

**Key Classes**:
- `CoralSpark` — **PUBLIC ENTRY POINT** for RelNode → Spark SQL conversion
- `IRRelToSparkRelTransformer` — Transforms Calcite RelNode to Spark-specific RelNode
- `SparkSqlDialect` (in `dialect/`) — **Calcite SqlDialect** for Spark SQL output
- `CoralToSparkSqlCallConverter` — Maps Coral/Hive functions to Spark functions
- `SparkSqlRewriter` — Post-processing of generated SQL
- **Packages**:
  - `dialect/SparkSqlDialect.java` — Extends Calcite's SqlDialect, handles SUBSTRING, UNNEST→EXPLODE, etc.
  - `transformers/` — UDF transformers (HiveUDFTransformer, TransportUDFTransformer)
  - `containers/SparkUDFInfo` — Metadata for Spark UDFs

---

## 2. KEY BASE CLASSES & INTERFACES (coral-common)

**Location**: `/Users/liwei/work/study/bigdata/coral/coral-common`

### Abstract Base: `ToRelConverter`
```
public abstract class ToRelConverter {
  protected abstract SqlRexConvertletTable getConvertletTable();
  protected abstract SqlValidator getSqlValidator();
  protected abstract SqlOperatorTable getOperatorTable();
  protected abstract SqlToRelConverter getSqlToRelConverter();
  protected abstract SqlNode toSqlNode(String sql, org.apache.hadoop.hive.metastore.api.Table);
  
  public RelRoot convertSqlToRelNode(String sql) { ... }
}
```
- **Every new dialect must extend this** to implement the 5 abstract methods
- Provides Calcite infrastructure (FrameworkConfig, CalciteCatalogReader, RelBuilder, Schema)
- Handles Hive metastore integration and CoralCatalog support

### Shared Components:
- `HiveRelBuilder` — Builds RelNodes with Hive semantics
- `HiveTypeSystem` — Hive/Spark type mapping
- `HiveToCoralTypeConverter` — Type conversion utilities
- `HiveCalciteTableAdapter`, `IcebergCalciteTableAdapter` — Table metadata adapters
- `CoralCatalog` — Unified interface for Hive/Iceberg tables
- `functions/Function` — Base class for function definitions

---

## 3. ENTRY POINTS & USER API

### Frontend Pattern (SQL → RelNode):
```java
// coral-hive
HiveToRelConverter converter = new HiveToRelConverter(coralCatalog);
RelNode irRelNode = converter.convertSqlToRelNode(hiveSql);

// coral-trino
TrinoToRelConverter converter = new TrinoToRelConverter(coralCatalog);
RelNode irRelNode = converter.convertSqlToRelNode(trinoSql);
```

### Backend Pattern (RelNode → Dialect SQL):
```java
// coral-spark
CoralSpark coralSpark = CoralSpark.create(irRelNode, hmsClient);
String sparkSql = coralSpark.getSparkSql();
List<String> baseTables = coralSpark.getBaseTables();
```

### Test Utilities:
- `com.linkedin.coral.common.ToRelConverterTestUtils` — Common test fixtures
- `com.linkedin.coral.spark.TestUtils` — Spark-specific test helpers
- Each module's test base setup: Hive metastore mocking, test table creation

---

## 4. FUNCTION MAPPING ARCHITECTURE

**Location**: `coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/functions/`

### Key Classes:
- **`StaticHiveFunctionRegistry`** — Central registry of all Hive functions
  - Singleton holding ~150+ built-in Hive functions (substr, concat, upper, lower, etc.)
  - UDFs registered with signature (name, return type, operand types)
  - Accessed via `StaticHiveFunctionRegistry.lookup(funcName)`

- **`HiveFunctionResolver`** — Resolves function calls during parsing
  - Uses StaticHiveFunctionRegistry + cached UDF registry
  - Handles function name matching, overload resolution
  - Produces `HiveFunction` instances

- **`HiveFunction`** — Individual function wrapper
  - Contains reference to Calcite `SqlOperator`
  - Implements `createCall()` to build Calcite `SqlCall` AST nodes
  - Special cases: CAST, CASE, aggregates

### For coral-gaussdb: Function Mapping Pattern
```
coral-gaussdb/src/main/java/com/linkedin/coral/gaussdb/gaussdb2rel/functions/
  ├── GaussDBFunctionResolver.java      (like HiveFunctionResolver)
  ├── StaticGaussDBFunctionRegistry.java (like StaticHiveFunctionRegistry)
  └── GaussDBFunction.java               (like HiveFunction - if needed)
```

### Spark Output Side (Function Rewriting):
**Location**: `coral-spark/src/main/java/com/linkedin/coral/spark/CoralToSparkSqlCallConverter.java`
- Transforms Hive → Spark function names (e.g., `substr` → `substring`)
- Uses `SqlCallTransformers` composition pattern
- Examples: HiveUDFTransformer, TransportUDFTransformer

---

## 5. GRADLE WIRING

### settings.gradle (Project Registration)
```gradle
include 'coral-common'
include 'coral-hive'
include 'coral-trino'
include 'coral-spark'
// Add: include 'coral-gaussdb'
```

### Module build.gradle (Dependencies)
```gradle
// Frontend like coral-hive
apply plugin: 'antlr'
dependencies {
  api project(':coral-common')        // Shared abstractions
  implementation deps.'antlr-runtime'  // For generated parser
  testImplementation deps.'hive'.'hive-exec-core'
}

// OR Frontend like coral-trino (uses external parser)
dependencies {
  implementation project(':coral-hive')  // Reuses Hive classes
  implementation project(':shading:coral-trino-parser')
  // (Trino parser is pre-built/shaded)
}

// Backend like coral-spark
dependencies {
  implementation project(':coral-hive')
  implementation project(':coral-schema')
  compileOnly deps.'spark'.'sql'
}
```

### Shared Dependencies (`gradle/dependencies.gradle`):
- Calcite 1.32.0
- Hadoop Hive
- Antlr 4.x
- TestNG, Mockito

---

## 6. TEST PATTERNS

**Typical Test Structure**:
```java
public class CoralSparkTest {
  private HiveConf conf;
  
  @BeforeClass
  public void beforeClass() throws Exception {
    conf = TestUtils.loadResourceHiveConf();  // Mock HMS
    TestUtils.initializeViews(conf);           // Create test tables/views
  }
  
  @Test
  public void testTranslation() {
    RelNode relNode = TestUtils.toRelNode("db", "view_name");
    CoralSpark spark = CoralSpark.create(relNode, hmsClient);
    String sql = spark.getSparkSql();
    assertEquals(expectedSpark, sql);  // String-in, string-out
  }
}
```

**Test Resources**:
- Hive metastore XML configs: `src/test/resources/`
- SQL files with test queries
- Test UDFs registered in StaticHiveFunctionRegistry
- TestNG for test execution

---

## 7. APACHE CALCITE INTEGRATION

**Core Calcite Classes Used Everywhere**:
- `org.apache.calcite.rel.RelNode` — Intermediate representation
- `org.apache.calcite.sql.SqlNode` — SQL AST
- `org.apache.calcite.sql.SqlOperatorTable` — Function registry
- `org.apache.calcite.sql.SqlValidator` — SQL validation
- `org.apache.calcite.sql2rel.SqlToRelConverter` — SQL → RelNode
- `org.apache.calcite.sql.SqlDialect` — SQL generation (backend)
- `org.apache.calcite.tools.RelBuilder` — Programmatic RelNode construction

### Dialect Configuration (Backend):
Each backend implements a **SqlDialect** subclass:
- **SparkSqlDialect** (coral-spark) — Overrides SUBSTRING, UNNEST→EXPLODE, etc.
- **TrinoSqlDialect** (coral-trino) — Identifier quoting, NULL collation
- **For coral-gaussdb**: Create `GaussDBSqlDialect extends SqlDialect` for PostgreSQL compatibility

### SQL Conformance (Frontend):
Each frontend specifies Calcite's **SqlConformance**:
- `HiveSqlConformance.HIVE_SQL` — Hive-specific rules
- `TrinoSqlConformance` — Trino-specific rules
- **For coral-gaussdb**: May reuse PostgreSQL conformance or create custom

---

## 8. DIRECTORY LAYOUT FOR coral-gaussdb

```
coral-gaussdb/
├── build.gradle                           # Add dependencies
├── src/
│   ├── main/java/com/linkedin/coral/gaussdb/
│   │   ├── gaussdb2rel/                   # GaussDB SQL → RelNode (frontend)
│   │   │   ├── GaussDBToRelConverter.java # PUBLIC ENTRY POINT
│   │   │   ├── GaussDBSqlToRelConverter.java
│   │   │   ├── GaussDBSqlValidator.java
│   │   │   ├── GaussDBSqlConformance.java
│   │   │   ├── functions/
│   │   │   │   ├── StaticGaussDBFunctionRegistry.java
│   │   │   │   └── GaussDBFunctionResolver.java
│   │   │   └── parsetree/                 # (if custom parser)
│   │   │       └── ParseTreeBuilder.java
│   │   └── rel2gaussdb/                   # RelNode → GaussDB SQL (backend)
│   │       ├── RelToGaussDBConverter.java # Optional, if bidirectional
│   │       └── GaussDBSqlDialect.java
│   └── test/java/com/linkedin/coral/gaussdb/
│       ├── gaussdb2rel/
│       │   └── GaussDBToRelConverterTest.java  # String-in, string-out tests
│       └── TestUtils.java
└── src/test/resources/
    ├── gaussdb-test-tables.sql              # Test table definitions
    └── hiveconf-test.xml                   # Mock Hive metastore
```

---

## KEY TAKEAWAYS FOR coral-gaussdb

1. **Inherit from `ToRelConverter`** → Implement 5 abstract methods (getConvertletTable, getSqlValidator, etc.)

2. **Use or create a parser**:
   - Option A: Use Calcite's built-in PostgreSQL dialect → simpler, less customization
   - Option B: Use existing PostgreSQL parser → reuse existing code
   - Option C: Custom ANTLR grammar → most control

3. **Create `StaticGaussDBFunctionRegistry`** → Map ~50-100 most common GaussDB functions to Calcite operators

4. **Create `GaussDBSqlDialect`** (if exporting RelNode → GaussDB) → Extend Calcite's SqlDialect, override `unparseCall()` for dialect-specific syntax

5. **Register in settings.gradle** → Add `include 'coral-gaussdb'`

6. **Test pattern**: Mock Hive metastore, create test views, convert SQL, assert equality

---

## FILE PATHS QUICK REFERENCE

| Component | Path |
|-----------|------|
| Base class | `coral-common/src/main/java/.../ToRelConverter.java` |
| Hive frontend | `coral-hive/src/main/java/.../HiveToRelConverter.java` |
| Trino frontend | `coral-trino/src/main/java/.../TrinoToRelConverter.java` |
| Spark backend | `coral-spark/src/main/java/.../CoralSpark.java` |
| Spark dialect | `coral-spark/src/main/java/.../dialect/SparkSqlDialect.java` |
| Type system | `coral-common/src/main/java/.../HiveTypeSystem.java` |
| Function registry | `coral-hive/src/main/java/.../functions/StaticHiveFunctionRegistry.java` |
| Common functions | `coral-common/src/main/java/.../functions/Function.java` |
| Hive test utils | `coral-hive/src/test/java/.../HiveToRelConverterTest.java` |
| Spark test utils | `coral-spark/src/test/java/.../CoralSparkTest.java` |

