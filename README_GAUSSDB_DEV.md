# Coral coral-gaussdb Development: Documentation Index

## 📚 Three Key Documents

This exploration generated **three comprehensive guides** to help you build the `coral-gaussdb` module:

### 1. **QUICK_REFERENCE.md** ⭐ START HERE
   - **Best for:** Quick navigation and debugging
   - **Contains:** 
     - Direct file path references (click-to-navigate)
     - Key entry points with line numbers
     - 5 abstract methods to override
     - GaussDB vs Hive differences table
     - Development roadmap (4 phases, 1-2 weeks)
   - **Length:** ~4 pages
   - **Use when:** You need to find a specific class, method, or quick answer

---

### 2. **CORAL_ARCHITECTURE_ANALYSIS.md** 📖 DETAILED REFERENCE
   - **Best for:** Understanding Coral's overall design
   - **Contains:**
     - Complete module layout (coral-common, coral-hive, coral-trino, coral-spark)
     - Key base classes & interfaces (5 core abstractions)
     - Entry points & API shapes (how users call the library)
     - Function mapping pattern (Hive `substr` → Spark `substring`)
     - Gradle wiring (how modules depend on each other)
     - Testing patterns (HiveToRelConverterTest example)
     - Apache Calcite integration details (v1.21.0.265 fork)
   - **Length:** ~8 pages
   - **Use when:** You're building your understanding of how Coral works

---

### 3. **CORAL_GAUSSDB_TEMPLATE.md** 🏗️ IMPLEMENTATION GUIDE
   - **Best for:** Actually building coral-gaussdb
   - **Contains:**
     - Architecture diagram (GaussDB SQL → RelNode → Spark SQL)
     - Directory structure (what files to create)
     - Phase-by-phase implementation plan (4 phases)
     - Code templates (GaussdbToRelConverter skeleton, etc.)
     - GaussDB-specific considerations (string concat `||`, arrays, JSON)
     - Compatibility strategy (reuse vs new)
     - Testing pattern examples
     - Comprehensive checklist
   - **Length:** ~10 pages
   - **Use when:** You're ready to start writing code

---

## 🚀 Getting Started (5-Minute Plan)

1. **Read QUICK_REFERENCE.md** (5 min)
   - Understand the 5 methods you must override
   - See the file paths to study

2. **Study HiveToRelConverter** (15 min)
   - Open: `/coral-hive/src/main/java/com/linkedin/coral/hive/hive2rel/HiveToRelConverter.java`
   - Read lines 48-133 (shows pattern for GaussdbToRelConverter)

3. **Skim CORAL_ARCHITECTURE_ANALYSIS.md** (10 min)
   - Focus on sections 1-3 (module layout, base classes, entry points)

4. **Open CORAL_GAUSSDB_TEMPLATE.md** (10 min)
   - Read "What Makes GaussDB Different from Hive" table
   - Review Phase 1 (core setup)

5. **Start coding**
   - Create `coral-gaussdb/` directory
   - Follow Phase 1 in CORAL_GAUSSDB_TEMPLATE.md

---

## 📊 Information Organization

```
QUICK_REFERENCE.md
├─ Entry Points to Study (6 key classes with line numbers)
├─ File Paths Reference (table, easy copy-paste)
├─ Gradle Configuration
├─ Calcite Key Classes (8 classes explained)
├─ GaussDB vs Hive (comparison table)
├─ Testing Quick Start
├─ Critical Methods to Override
├─ Development Roadmap
└─ Debugging Tips

CORAL_ARCHITECTURE_ANALYSIS.md
├─ Module Layout (coral-common, coral-hive, coral-trino, coral-spark)
├─ Key Classes (entry points, parsers, function mapping)
├─ Build Config (gradle patterns)
├─ Testing Pattern (string-in, string-out)
├─ Apache Calcite Integration
└─ Template for coral-gaussdb

CORAL_GAUSSDB_TEMPLATE.md
├─ Architecture Diagram
├─ Directory Structure (exact files to create)
├─ Phase 1: Module Files
├─ Phase 2: Key Classes (with code stubs)
├─ Phase 3: GaussDB Considerations
├─ Phase 4: Implementation Order
├─ Phase 5: Testing Pattern
└─ Checklist (14 items)
```

---

## 🎯 Development Checklist (From CORAL_GAUSSDB_TEMPLATE.md)

### Phase 1: Create Core Module Files (1 day)
- [ ] Add `include 'coral-gaussdb'` to `settings.gradle`
- [ ] Create `coral-gaussdb/build.gradle` with dependencies
- [ ] Create `coral-gaussdb/src/main/java/.../gaussdb2rel/` structure

### Phase 2: Implement Parser & Validator (2-3 days)
- [ ] `GaussdbSqlConformance.java` — SQL syntax rules
- [ ] `GaussdbSqlValidator.java` — Extends HiveSqlValidator
- [ ] `GaussdbSqlToRelConverter.java` — Calcite bridge
- [ ] Test basic SELECT, WHERE, FROM parsing

### Phase 3: Function Registry (2-3 days)
- [ ] `StaticGaussdbFunctionRegistry.java` — 200+ functions
- [ ] Map string concat operator `||`
- [ ] Map array functions (PostgreSQL arrays)
- [ ] Map JSON functions

### Phase 4: Integration & Testing (1-2 days)
- [ ] `GaussdbToRelConverterTest.java` — Comprehensive tests
- [ ] End-to-end test: GaussDB SQL → RelNode → Spark SQL
- [ ] Performance tests

---

## 🔗 Direct File References

### Must Read (in order)
1. **HiveToRelConverter** → `coral-hive/src/main/java/.../hive/hive2rel/HiveToRelConverter.java`
   - Your template for GaussdbToRelConverter
2. **ToRelConverter** → `coral-common/src/main/java/.../common/ToRelConverter.java`
   - Abstract base class (understand the 5 methods)
3. **StaticHiveFunctionRegistry** → `coral-hive/src/main/java/.../hive/hive2rel/functions/StaticHiveFunctionRegistry.java`
   - Your template for StaticGaussdbFunctionRegistry
4. **HiveToRelConverterTest** → `coral-hive/src/test/java/.../hive/hive2rel/HiveToRelConverterTest.java`
   - Your testing template

### Reference
- **CoralSpark** → `coral-spark/src/main/java/.../spark/CoralSpark.java`
  - Shows how backends consume RelNode IR
- **FunctionRegistry** → `coral-common/src/main/java/.../common/functions/FunctionRegistry.java`
  - Interface you'll implement

---

## 💡 Key Insights

### What coral-gaussdb Does
```
GaussDB SQL + PostgreSQL Functions + Arrays + JSON
        ↓
   Parse with Calcite
        ↓
GaussDB Functions → Calcite SqlOperators
        ↓
   Generate RelNode IR (dialect-agnostic)
        ↓
   Feed to coral-spark or other backends
        ↓
Spark SQL / Other Output
```

### GaussDB-Specific Challenges
1. **String concatenation** — Use `||` operator (not `concat()`)
2. **Array syntax** — Parse `ARRAY[1,2,3]` (not `ARRAY(1,2,3)`)
3. **JSON support** — JSONB is first-class type (map to STRING in IR)
4. **PostgreSQL functions** — ~200+ functions not in Hive (array_agg, string_agg, date_trunc, etc.)
5. **Type system** — Map PostgreSQL types to Coral types

### Reuse Strategy
- **ParseTreeBuilder** — Can reuse Hive's parser (GaussDB/PostgreSQL-compatible)
- **CoralConvertletTable** — Reuse from Hive
- **DaliOperatorTable** — Extend with GaussDB operators
- **Function Registry** — Create new, but follow Hive pattern
- **RelBuilder** — Reuse HiveRelBuilder

---

## ❓ FAQ

**Q: Do I need to write a parser?**
A: No! GaussDB is PostgreSQL-compatible. You can reuse Hive's ANTLR parser or use Trino's parser (which already handles PostgreSQL syntax).

**Q: How many functions do I need to map?**
A: Start with ~50 core functions, then expand. StaticHiveFunctionRegistry has ~100+. Coral can skip unknown functions with graceful degradation.

**Q: Will coral-gaussdb work with coral-spark?**
A: Yes! RelNode IR is dialect-agnostic. Once you generate RelNode from GaussDB SQL, coral-spark will convert it to Spark SQL automatically.

**Q: What if GaussDB has a function Spark doesn't support?**
A: Calcite will preserve it as an unrecognized function call. When coral-spark tries to generate Spark SQL, it will either:
   1. Map it to equivalent Spark function
   2. Raise an error (if truly unsupported)
   3. Keep it as-is (if Spark can parse it)

**Q: How long will this take?**
A: 1-2 weeks for a functional module. Start with ~100 functions, test end-to-end, then expand.

---

## 📞 Support Resources

### If You Get Stuck
1. **Check QUICK_REFERENCE.md** for debugging tips
2. **Look at HiveToRelConverter** — it's your template
3. **Examine StaticHiveFunctionRegistry** — for function registry pattern
4. **Look at tests** — HiveToRelConverterTest shows expected patterns
5. **Calcite docs** — For SqlOperator, SqlConformance, SqlValidator

### External Resources
- **GaussDB/openGauss docs** — Function reference, type system
- **PostgreSQL docs** — GaussDB is PostgreSQL-compatible
- **Calcite docs** — SqlNode, SqlOperator, RelNode
- **Coral README** — High-level project overview

---

## 📈 Success Metrics

Your implementation is working when:
1. ✅ `gradle build coral-gaussdb` succeeds
2. ✅ Basic SELECT parsing works
3. ✅ Function resolution works (e.g., `substr` → Calcite operator)
4. ✅ GaussDB SQL → RelNode conversion completes
5. ✅ RelNode → Spark SQL works end-to-end
6. ✅ Test suite passes (HiveToRelConverterTest-style tests)

---

## 🎓 Learning Path

**Day 1:** Understanding
- Read QUICK_REFERENCE.md (30 min)
- Study HiveToRelConverter (1 hour)
- Skim CORAL_ARCHITECTURE_ANALYSIS.md (30 min)

**Day 2:** Planning
- Read CORAL_GAUSSDB_TEMPLATE.md (1 hour)
- Map GaussDB functions to Calcite operators (spreadsheet, 2 hours)
- Plan directory structure

**Day 3-5:** Implementation
- Phase 1 (core module setup) - 1 day
- Phase 2 (parsing & validation) - 2 days
- Phase 3 (function registry) - 2 days
- Phase 4 (testing) - 1 day

---

## 📝 Notes

- These documents were generated by analyzing Coral codebase on 2026/04/28
- All line numbers and file paths are accurate as of that date
- Coral version: LinkedIn fork of Calcite 1.21.0.265
- Hive version: 1.2.2
- Spark versions supported: 2.4.0, 3.1.1, 3.5.0

---

**Start with QUICK_REFERENCE.md, then move to CORAL_GAUSSDB_TEMPLATE.md when ready to code!**
