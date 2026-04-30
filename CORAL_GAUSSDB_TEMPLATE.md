# coral-gaussdb Module Implementation Guide

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────┐
│ GaussDB SQL Input String                                    │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼
┌─────────────────────────────────────────────────────────────┐
│ coral-gaussdb (Frontend: GaussDB SQL → Calcite RelNode)     │
├─────────────────────────────────────────────────────────────┤
│ GaussdbToRelConverter (extends ToRelConverter)              │
│ ├─ GaussdbSqlConformance (SQL syntax rules)                 │
│ ├─ GaussdbSqlValidator (validates GaussDB SQL)             │
│ ├─ GaussdbFunctionRegistry (maps GaussDB → Calcite ops)    │
│ └─ GaussdbSqlToRelConverter (SQL→RelNode bridge)           │
└────────────────────────┬────────────────────────────────────┘
                         │
                         ▼ (RelNode IR)
┌─────────────────────────────────────────────────────────────┐
│ coral-common (Shared IR & Abstractions)                     │
├─────────────────────────────────────────────────────────────┤
│ ├─ RelNode (Calcite relational algebra)                     │
│ ├─ TypeSystem, FunctionRegistry                            │
│ └─ CoralCatalog (table metadata)                           │
└────────────────────────┬────────────────────────────────────┘
                         │
                  ┌──────┴──────┐
                  ▼             ▼
        ┌──────────────────┐  ┌──────────────────┐
        │ coral-spark      │  │ other backends   │
        │ (RelNode→Spark)  │  │ (future)         │
        └──────────────────┘  └──────────────────┘
                  │
                  ▼
        Spark SQL Output
```

## Directory Structure

```
coral-gaussdb/
├── build.gradle                          # Gradle config (depends on coral-common)
├── src/
│   ├── main/java/com/linkedin/coral/gaussdb/
│   │   └── gaussdb2rel/                 # Frontend: GaussDB SQL → RelNode
│   │       ├── GaussdbToRelConverter.java     (Main entry point)
│   │       ├── GaussdbSqlConformance.java    (SQL dialect rules)
│   │       ├── GaussdbSqlValidator.java      (SQL validation)
│   │       ├── GaussdbSqlToRelConverter.java (Calcite bridge)
│   │       ├── GaussdbSqlNodeToCoralSqlNodeConverter.java
│   │       ├── functions/
│   │       │   ├── GaussdbFunctionRegistry.java       (ALL GaussDB functions)
│   │       │   └── StaticGaussdbFunctionRegistry.java (Hard-coded registry)
│   │       └── parsetree/
│   │           ├── ParseTreeBuilder.java    (SQL text → SqlNode AST)
│   │           ├── GaussdbParserDriver.java (Uses existing parser)
│   │           └── ParserVisitorContext.java
│   │
│   └── test/java/com/linkedin/coral/gaussdb/
│       ├── GaussdbToRelConverterTest.java
│       └── resources/
│           └── gaussdb_test_schema.hql     (Test tables)
```

## Phase 1: Create Core Module Files

### 1.1 settings.gradle
Add to `/settings.gradle`:
```gradle
include 'coral-gaussdb'
```

### 1.2 coral-gaussdb/build.gradle
```gradle
dependencies {
  api project(path: ':coral-common')
  implementation project(':coral-hive')  // Reuse Hive parser infrastructure
  
  testImplementation deps.'hive'.'hive-exec-core'
  testImplementation deps.'hadoop'.'hadoop-mapreduce-client-core'
  testImplementation deps.'kryo'
}
```

## Phase 2: Key Classes to Implement

### 2.1 GaussdbToRelConverter.java
```java
public class GaussdbToRelConverter extends ToRelConverter {
  private final ParseTreeBuilder parseTreeBuilder;
  private SqlToRelConverter sqlToRelConverter;
  private final GaussdbFunctionResolver functionResolver;
  private final SqlValidator sqlValidator;

  public GaussdbToRelConverter(CoralCatalog catalog) {
    super(catalog);
    this.functionResolver = new GaussdbFunctionResolver(
        new StaticGaussdbFunctionRegistry(), 
        new ConcurrentHashMap<>()
    );
    this.parseTreeBuilder = new ParseTreeBuilder(functionResolver);
  }

  @Override
  protected SqlValidator getSqlValidator() { ... }
  @Override
  protected SqlOperatorTable getOperatorTable() { ... }
  @Override
  protected SqlToRelConverter getSqlToRelConverter() { ... }
}
```

### 2.2 GaussdbFunctionRegistry.java
Maps GaussDB functions to Calcite operators. Key functions:
- `abs()`, `ceil()`, `floor()` → Standard Calcite operators
- `substr()` → `SUBSTRING`
- `concat()` → `||` or Calcite CONCAT
- `upper()`, `lower()` → Standard Calcite
- **GaussDB-specific:** `pg_typeof()`, `arrayAgg()`, `jsonbExtract()`, etc.
  - Map to custom GaussDB operators or Calcite equivalents

Example entry:
```java
registerBuiltInFunction("substr", SqlStdOperatorTable.SUBSTRING,
    family(SqlTypeFamily.STRING),
    family(SqlTypeFamily.INTEGER),
    family(SqlTypeFamily.INTEGER));
```

### 2.3 GaussdbSqlConformance.java
Defines SQL syntax rules:
```java
public class GaussdbSqlConformance extends SqlConformanceImpl {
  public static final SqlConformance GAUSSDB_SQL = new GaussdbSqlConformance();
  // Allow GaussDB-specific features:
  // - Column aliases in WHERE (not ANSI-compliant)
  // - String concatenation with ||
  // - Other PostgreSQL/GaussDB extensions
}
```

### 2.4 StaticGaussdbFunctionRegistry.java
Comprehensive registry of GaussDB functions (~200+ functions):

**Categories:**
1. **String Functions:** `substr`, `length`, `trim`, `ltrim`, `rtrim`, `upper`, `lower`, `concat`, `split_part`, etc.
2. **Math Functions:** `abs`, `ceil`, `floor`, `round`, `sqrt`, `power`, `exp`, `log`, etc.
3. **Date Functions:** `now`, `current_date`, `date_trunc`, `extract`, `date_add`, etc.
4. **Aggregate Functions:** `sum`, `avg`, `count`, `min`, `max`, `array_agg`, `string_agg`, etc.
5. **Type Casting:** `cast`, `to_char`, `to_number`, `to_timestamp`, etc.
6. **Array Functions (PostgreSQL arrays):** `array_append`, `array_concat`, `array_length`, etc.
7. **JSON Functions:** `json_extract`, `jsonb_extract`, `json_array_elements`, etc.

## Phase 3: GaussDB-Specific Considerations

### What Makes GaussDB Different from Hive?

| Feature | Hive | GaussDB | Handling |
|---------|------|---------|----------|
| **String concat** | `concat(a,b)` | `a \|\| b` (operator) | Add operator to function registry |
| **Substring indexing** | 1-based | 1-based (PostgreSQL-compatible) | Same as Hive ✓ |
| **Type system** | Strict Hive types | PostgreSQL types + extensions | Create `GaussdbToCoralTypeConverter` |
| **Array literals** | `ARRAY(1,2,3)` | `ARRAY[1,2,3]` | Handle in parser |
| **JSON** | Limited | JSONB (first-class type) | Map to STRING for IR, handle in output |
| **Date arithmetic** | Interval functions | `DATE + INTERVAL` syntax | Add custom operator |
| **NULL handling** | Standard SQL | PostgreSQL semantics | May need custom rules |
| **Window functions** | Limited | Full PostgreSQL support | Leverage Calcite's window frame support |

### Compatibility Strategy
1. **Reuse Hive Operator Table** — Most core operators (JOIN, GROUP BY, ORDER BY) are standard
2. **Add GaussDB extensions** — New operators/functions unique to GaussDB
3. **Map to Calcite equivalents** — Convert GaussDB-specific syntax to Calcite operators
4. **Normalize to IR** — IR is dialect-agnostic, so downstream backends (Spark) work unchanged

## Phase 4: Implementation Order

1. **Week 1:** Core structure
   - Create module directory + build.gradle
   - Implement `GaussdbToRelConverter` skeleton
   - Setup basic test infrastructure

2. **Week 2:** Parser & validator
   - Implement `GaussdbSqlConformance`
   - Implement `GaussdbSqlValidator` (subclass HiveSqlValidator)
   - Test basic SQL parsing (SELECT, WHERE, FROM)

3. **Week 3:** Function registry
   - List all ~200+ GaussDB functions
   - Implement `StaticGaussdbFunctionRegistry`
   - Add GaussDB-specific operators (e.g., string concat `||`)

4. **Week 4:** Edge cases & testing
   - Handle GaussDB type system quirks
   - Comprehensive test suite
   - Integration test: GaussDB SQL → Spark SQL (full pipeline)

## Phase 5: Testing Pattern

**GaussdbToRelConverterTest.java:**
```java
@BeforeClass
public static void beforeClass() throws IOException, HiveException {
  conf = TestUtils.loadResourceHiveConf();
  ToRelConverterTestUtils.setup(conf);
}

@Test
public void testSimpleSelect() {
  GaussdbToRelConverter converter = new GaussdbToRelConverter(catalog);
  RelNode rel = converter.convertSql("SELECT * FROM table_name");
  assertNotNull(rel);
}

@Test
public void testStringConcatenation() {
  GaussdbToRelConverter converter = new GaussdbToRelConverter(catalog);
  RelNode rel = converter.convertSql("SELECT 'hello' || ' ' || 'world'");
  // Verify RelNode contains CONCAT operator (or equivalent)
}

@Test
public void testArrayLiterals() {
  GaussdbToRelConverter converter = new GaussdbToRelConverter(catalog);
  RelNode rel = converter.convertSql("SELECT ARRAY[1,2,3] as arr");
  // Verify array literal handling
}
```

## Resources

1. **GaussDB/openGauss SQL Reference:** Document function signatures, type semantics
2. **Calcite Documentation:** `SqlOperator`, `SqlConformance`, `SqlValidator`
3. **Coral Codebase:**
   - Reference: `/coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/`
   - Copy patterns from `HiveToRelConverter`, `StaticHiveFunctionRegistry`
4. **PostgreSQL Docs:** GaussDB is PostgreSQL-compatible; check PG function library

## Summary: What coral-gaussdb Does

```
Input:  GaussDB SQL
        ├─ String concat: 'a' || 'b'
        ├─ Arrays: ARRAY[1,2,3]
        ├─ JSON: jsonb_extract(obj, 'key')
        ├─ PostgreSQL functions: date_trunc(), string_agg(), etc.
        └─ GaussDB extensions: custom operators, types

↓ (via GaussdbToRelConverter)

Output: Calcite RelNode IR (dialect-agnostic)
        ├─ CONCAT operator (from || mapping)
        ├─ ARRAY constructor
        ├─ Standard aggregate/scalar functions
        └─ Type information preserved

↓ (fed to coral-spark or other backends)

Final:  Spark SQL / other target dialect
```

## Checklist

- [ ] Create `coral-gaussdb/` directory
- [ ] Add to `settings.gradle`
- [ ] Create `build.gradle`
- [ ] Implement `GaussdbToRelConverter`
- [ ] Implement `GaussdbSqlConformance`
- [ ] Implement `GaussdbSqlValidator`
- [ ] Implement `StaticGaussdbFunctionRegistry` (start with core functions)
- [ ] Implement parser/visitor classes
- [ ] Write comprehensive test suite
- [ ] Document GaussDB-specific function mappings
- [ ] Integration test: GaussDB → Spark SQL pipeline
- [ ] Performance testing on real queries
