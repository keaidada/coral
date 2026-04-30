# Coral Project: Quick Reference for coral-gaussdb Development

## Key Entry Points to Study (in order)

### 1. **Frontend Example: HiveToRelConverter**
📍 `/coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/HiveToRelConverter.java` (lines 48-133)

What to learn:
- How to extend `ToRelConverter`
- Managing ParseTreeBuilder, FunctionResolver, SqlValidator
- Method override pattern for `getOperatorTable()`, `getSqlValidator()`, etc.

---

### 2. **Abstract Base: ToRelConverter**
📍 `/coral-common/src/main/java/com/linkedin/coral/common/ToRelConverter.java` (lines 59-130)

What to learn:
- 5 abstract methods you must override in coral-gaussdb
- Calcite framework setup (SqlValidator, RelBuilder, etc.)
- Catalog initialization

---

### 3. **Function Registry Pattern: StaticHiveFunctionRegistry**
📍 `/coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/functions/StaticHiveFunctionRegistry.java` (lines 59-200)

What to learn:
- How to register functions (name → SqlOperator mapping)
- Using Calcite's `ReturnTypes` and `OperandTypes` for type inference
- Structure for ~100+ functions

---

### 4. **SQL Validator: HiveSqlValidator**
📍 `/coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/HiveSqlValidator.java`

What to learn:
- Subclass for dialect-specific validation
- Setting up SqlConformance

---

### 5. **Backend Example: CoralSpark**
📍 `/coral-spark/src/main/java/com/linkedin/coral/spark/CoralSpark.java` (lines 43-80)

What to learn:
- How RelNode IR is consumed by backends
- Pattern for IR → SQL generation
- SparkSqlDialect usage

---

### 6. **SQL Dialect: SparkSqlDialect**
📍 `/coral-spark/src/main/java/com/linkedin/coral/spark/dialect/SparkSqlDialect.java` (lines 38-80)

What to learn:
- How to extend Calcite's SqlDialect
- Overriding `unparseCall()` for dialect-specific syntax

---

## File Paths Reference

| Component | File Path |
|-----------|-----------|
| **ToRelConverter (base)** | `coral-common/src/main/java/.../common/ToRelConverter.java` |
| **FunctionRegistry (interface)** | `coral-common/src/main/java/.../common/functions/FunctionRegistry.java` |
| **HiveToRelConverter** | `coral-hive/src/main/java/.../hive/hive2rel/HiveToRelConverter.java` |
| **StaticHiveFunctionRegistry** | `coral-hive/src/main/java/.../hive/hive2rel/functions/StaticHiveFunctionRegistry.java` |
| **TrinoToRelConverter** | `coral-trino/src/main/java/.../trino/trino2rel/TrinoToRelConverter.java` |
| **CoralSpark** | `coral-spark/src/main/java/.../spark/CoralSpark.java` |
| **SparkSqlDialect** | `coral-spark/src/main/java/.../spark/dialect/SparkSqlDialect.java` |
| **HiveToRelConverterTest** | `coral-hive/src/test/java/.../hive/hive2rel/HiveToRelConverterTest.java` |
| **CoralSparkTest** | `coral-spark/src/test/java/.../spark/CoralSparkTest.java` |

---

## Gradle Configuration

### Add to settings.gradle
```gradle
include 'coral-gaussdb'
```

### coral-gaussdb/build.gradle
```gradle
dependencies {
  api project(path: ':coral-common')
  implementation project(':coral-hive')
  
  testImplementation deps.'hive'.'hive-exec-core'
  testImplementation deps.'hadoop'.'hadoop-mapreduce-client-core'
  testImplementation deps.'kryo'
}
```

---

## Calcite Key Classes

| Class | Purpose | Coral Usage |
|-------|---------|------------|
| `SqlNode` | SQL AST (parsed SQL) | Intermediate representation |
| `SqlOperator` | Function/operator definition | Function registry entries |
| `RelNode` | Relational algebra IR | Core interchange format |
| `SqlValidator` | SQL type checking | Validates dialect-specific rules |
| `SqlConformance` | SQL dialect syntax rules | Define GaussDB syntax |
| `SqlDialect` | SQL generation (unparse) | Backend output formatting |
| `SqlToRelConverter` | SQL→RelNode | Bridge between SQL and IR |
| `SqlOperatorTable` | Registry of operators | Maps names to operators |

---

## GaussDB vs Hive: Key Differences

| Aspect | Hive | GaussDB | Action for coral-gaussdb |
|--------|------|---------|-------------------------|
| **String concat** | `concat('a','b')` | `'a' \|\| 'b'` | Add custom operator for `\|\|` |
| **Array syntax** | `ARRAY(1,2,3)` | `ARRAY[1,2,3]` | Map both to Calcite ARRAY |
| **Substring** | `substr()` | `substr()` | Same, reuse mapping |
| **JSON support** | Minimal | JSONB (first-class) | Map JSONB to STRING in IR |
| **Type system** | Hive types | PostgreSQL types | Create type converter |
| **Date arithmetic** | Function-based | `DATE + INTERVAL` syntax | Add operator for `+` |

---

## Testing Quick Start

```java
// GaussdbToRelConverterTest.java template
@BeforeClass
public static void beforeClass() throws IOException, HiveException {
  conf = TestUtils.loadResourceHiveConf();
  ToRelConverterTestUtils.setup(conf);
}

@Test
public void testSimpleSelect() {
  GaussdbToRelConverter converter = new GaussdbToRelConverter(catalog);
  RelNode rel = converter.convertSql("SELECT * FROM my_table");
  assertNotNull(rel);
  // Verify RelNode structure
}
```

---

## Critical Methods to Override in GaussdbToRelConverter

1. **`getConvertletTable()`**
   - Returns `CoralConvertletTable` (shared from Hive)
   - Maps SQL expressions → RexNode

2. **`getSqlValidator()`**
   - Returns `GaussdbSqlValidator` instance
   - Validates GaussDB SQL semantics

3. **`getOperatorTable()`**
   - Returns `ChainedSqlOperatorTable` combining:
     - Standard Calcite operators
     - GaussDB-specific operators

4. **`getSqlToRelConverter()`**
   - Returns configured Calcite `SqlToRelConverter`
   - Bridges SQL AST → RelNode IR

5. **`toSqlNode(String sql, Table hiveView)`**
   - Uses `ParseTreeBuilder` to parse SQL text
   - Returns Calcite `SqlNode`

---

## Development Roadmap

### Phase 1: Skeleton (1 day)
- Create `coral-gaussdb/` directory
- Add to `settings.gradle`
- Create `build.gradle`
- Create empty `GaussdbToRelConverter.java`
- Verify project builds

### Phase 2: Basic Parsing (2-3 days)
- Implement `GaussdbSqlConformance`
- Implement `GaussdbSqlValidator`
- Implement `GaussdbSqlToRelConverter`
- Test basic SELECT parsing

### Phase 3: Function Registry (2-3 days)
- Create `StaticGaussdbFunctionRegistry`
- Register core functions (100+ functions)
- Add GaussDB-specific operators (`||`, etc.)
- Test function resolution

### Phase 4: Integration & Testing (1-2 days)
- Write comprehensive test suite
- Test end-to-end: GaussDB SQL → RelNode → Spark SQL
- Document GaussDB function mappings

---

## Debugging Tips

### See the RelNode IR
```java
String relPlan = RelOptUtil.toString(relNode);
System.out.println(relPlan);
```

### See the Calcite Plan (optimized)
```java
String explainPlan = RelOptUtil.toString(optimizedRel, SqlExplainFormat.TEXT, SqlExplainLevel.ALL_ATTRIBUTES);
System.out.println(explainPlan);
```

### Trace Function Resolution
In `GaussdbToRelConverter`:
```java
functionResolver.lookup("substr") // Returns Collection<Function>
```

---

## Key Interfaces & Contracts

### FunctionRegistry.lookup(String functionName)
```java
Collection<Function> lookup(String name);
// Returns all overloads of a function
// Empty collection if not found
```

### Function (interface)
```java
interface Function {
  String getName();
  SqlOperator getOperator();
  RelDataType getReturnType(...);
}
```

---

## Useful Calcite Imports

```java
import org.apache.calcite.sql.SqlNode;
import org.apache.calcite.sql.SqlOperator;
import org.apache.calcite.sql.SqlOperatorTable;
import org.apache.calcite.sql.fun.SqlStdOperatorTable;
import org.apache.calcite.sql.type.ReturnTypes;
import org.apache.calcite.sql.type.OperandTypes;
import org.apache.calcite.sql.type.SqlTypeFamily;
import org.apache.calcite.rel.RelNode;
import org.apache.calcite.sql.validate.SqlValidator;
import org.apache.calcite.sql.validate.SqlConformance;
```

---

## Summary: Data Flow

```
GaussDB SQL Input
    ↓
ParseTreeBuilder (uses existing parser)
    ↓
SqlNode (Calcite AST)
    ↓
GaussdbSqlValidator (validates)
    ↓
SqlToRelConverter (SQL AST → RelNode)
    ↓
GaussdbFunctionRegistry (resolves functions)
    ↓
RelNode IR (dialect-agnostic)
    ↓
coral-spark (or other backend)
    ↓
Spark SQL Output
```

---

## Files Saved in /coral/ Directory

1. **CORAL_ARCHITECTURE_ANALYSIS.md** — Detailed architecture report (this session)
2. **CORAL_GAUSSDB_TEMPLATE.md** — Implementation guide for coral-gaussdb
3. **QUICK_REFERENCE.md** — This file

Start with QUICK_REFERENCE.md for navigation, then dive into CORAL_ARCHITECTURE_ANALYSIS.md for details.
