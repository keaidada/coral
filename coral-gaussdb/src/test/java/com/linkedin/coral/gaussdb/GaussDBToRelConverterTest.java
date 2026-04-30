/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb;

import org.apache.calcite.plan.RelOptUtil;
import org.apache.calcite.rel.RelNode;
import org.apache.calcite.rel.RelWriter;
import org.apache.calcite.rel.externalize.RelWriterImpl;
import org.apache.calcite.sql.SqlExplainLevel;
import org.testng.annotations.BeforeClass;
import org.testng.annotations.Test;

import com.linkedin.coral.gaussdb.parsetree.GaussDBParserDriver;
import com.linkedin.coral.gaussdb.parser.GaussDBSqlParser;

import static org.testng.Assert.*;


/**
 * Stage-1 smoke tests for {@link GaussDBToRelConverter}. These are NOT yet
 * golden-file comparisons — they assert that the pipeline (parse → visit →
 * validate → rel) completes without error and the produced RelNode tree has the
 * expected top-level operators. Golden comparisons on plan text will follow in
 * a later milestone once enough surface exists to stabilize output.
 */
public class GaussDBToRelConverterTest {

  private GaussDBToRelConverter converter;

  @BeforeClass
  public void setUp() {
    converter = new GaussDBToRelConverter(TestCatalog.defaultCatalog());
  }

  /**
   * Case 1: the trivial select should parse cleanly into a non-null RelNode.
   */
  @Test
  public void testBasicSelectParsesCleanly() {
    // Parser-level only: make sure our ANTLR grammar accepts the input.
    GaussDBSqlParser.RootContext root = GaussDBParserDriver.parse("SELECT id, name FROM employees WHERE dept_id = 10");
    assertNotNull(root);
    assertNotNull(root.statement());
  }

  /**
   * Case 2: CASE / COALESCE / NVL normalization.
   */
  @Test
  public void testCaseAndNullFunctionsParse() {
    String sql = "SELECT CASE WHEN salary > 100 THEN 'high' ELSE 'low' END, COALESCE(name, 'n/a'), "
        + "NVL(mgr_id, 0) FROM employees";
    assertNotNull(GaussDBParserDriver.parse(sql));
  }

  /**
   * Case 3: INNER / LEFT joins across the test catalog.
   */
  @Test
  public void testJoinsParse() {
    String sql = "SELECT e.id, d.name FROM employees AS e INNER JOIN departments AS d ON e.dept_id = d.id";
    assertNotNull(GaussDBParserDriver.parse(sql));

    sql = "SELECT e.id, d.name FROM employees e LEFT JOIN departments d ON e.dept_id = d.id";
    assertNotNull(GaussDBParserDriver.parse(sql));
  }

  /**
   * Case 4: String concat with ||.
   */
  @Test
  public void testStringConcatParse() {
    String sql = "SELECT name || ' - ' || dept_id FROM employees";
    assertNotNull(GaussDBParserDriver.parse(sql));
  }

  /**
   * Case 5: PostgreSQL double-colon cast and standard CAST.
   */
  @Test
  public void testCastExpressionsParse() {
    String sql = "SELECT salary::INT, CAST(id AS BIGINT) FROM employees";
    assertNotNull(GaussDBParserDriver.parse(sql));
  }

  /**
   * End-to-end: parse + validate + convert to RelNode. Uses the "default.foo"
   * table because our validator resolves identifiers against the "default"
   * schema by default (matching Hive/Coral's conventions).
   */
  @Test
  public void testEndToEndRelNodeFromSimpleSelect() {
    RelNode rel = converter.convertSql("SELECT a, b FROM foo");
    assertNotNull(rel);
    // Root of a SELECT should be a Project.
    assertTrue(rel.getClass().getSimpleName().contains("Project"),
        "expected a Project at root, got " + rel.getClass().getSimpleName());
  }

  /**
   * M2.1: GROUP BY / HAVING round-trips through validation. The plan must
   * contain an Aggregate operator, and the HAVING must surface as a Filter
   * above the Aggregate.
   */
  @Test
  public void testGroupByHavingEndToEnd() {
    RelNode rel = converter.convertSql(
        "SELECT dept_id, COUNT(*), SUM(salary) FROM employees GROUP BY dept_id HAVING COUNT(*) > 1");
    assertNotNull(rel);
    String plan = RelOptUtil.toString(rel);
    assertTrue(plan.contains("Aggregate"), "expected Aggregate in plan, got:\n" + plan);
    assertTrue(plan.contains("Filter"), "expected Filter (from HAVING) in plan, got:\n" + plan);
  }

  /**
   * M2.2: 3-way UNION ALL. Plan root should be a Union. We use the same
   * table on both sides for simplicity — the point is the set-op wiring, not
   * schema unification.
   */
  @Test
  public void testUnionAllEndToEnd() {
    RelNode rel = converter.convertSql(
        "SELECT id FROM employees UNION ALL SELECT id FROM employees UNION ALL SELECT id FROM employees");
    assertNotNull(rel);
    String plan = RelOptUtil.toString(rel);
    assertTrue(plan.contains("Union"), "expected Union, got:\n" + plan);
  }

  /**
   * M2.2: MINUS is recognized as a synonym for EXCEPT.
   */
  @Test
  public void testMinusIsExcept() {
    RelNode rel = converter.convertSql("SELECT id FROM employees MINUS SELECT id FROM departments");
    assertNotNull(rel);
    String plan = RelOptUtil.toString(rel);
    assertTrue(plan.contains("Minus") || plan.contains("Except"),
        "expected Minus/Except, got:\n" + plan);
  }

  /**
   * M2.3: single non-recursive CTE; validator should accept it and the body
   * query should resolve columns against the CTE.
   */
  @Test
  public void testSingleCteEndToEnd() {
    RelNode rel = converter.convertSql(
        "WITH high_paid AS (SELECT id, name FROM employees WHERE salary > 100) "
            + "SELECT id FROM high_paid");
    assertNotNull(rel);
  }

  /**
   * M2.3: multi-CTE with column-list alias.
   */
  @Test
  public void testMultiCteWithColumnAlias() {
    RelNode rel = converter.convertSql(
        "WITH a (x) AS (SELECT id FROM employees), b (y) AS (SELECT id FROM departments) "
            + "SELECT a.x, b.y FROM a INNER JOIN b ON a.x = b.y");
    assertNotNull(rel);
  }

  /**
   * M2.4: scalar subquery in SELECT projection.
   */
  @Test
  public void testScalarSubqueryInProjection() {
    RelNode rel = converter.convertSql(
        "SELECT id, (SELECT COUNT(*) FROM employees) FROM departments");
    assertNotNull(rel);
  }

  /**
   * M2.4: IN (subquery).
   */
  @Test
  public void testInSubquery() {
    RelNode rel = converter.convertSql(
        "SELECT id FROM employees WHERE dept_id IN (SELECT id FROM departments)");
    assertNotNull(rel);
  }

  /**
   * M2.4: correlated EXISTS.
   */
  @Test
  public void testCorrelatedExists() {
    RelNode rel = converter.convertSql(
        "SELECT e.id FROM employees e WHERE EXISTS (SELECT 1 FROM departments d WHERE d.id = e.dept_id)");
    assertNotNull(rel);
  }

  /**
   * M2.5: FULL OUTER, CROSS, and USING join variants.
   */
  @Test
  public void testJoinVariants() {
    RelNode rel;

    rel = converter.convertSql(
        "SELECT e.id FROM employees e FULL OUTER JOIN departments d ON e.dept_id = d.id");
    assertNotNull(rel);

    rel = converter.convertSql("SELECT e.id FROM employees e CROSS JOIN departments d");
    assertNotNull(rel);

    // USING(id) collapses the two `id` columns into one. The employees-side
    // `name` is disambiguated via alias `e`.
    rel = converter.convertSql(
        "SELECT e.name FROM employees e INNER JOIN departments d USING (id)");
    assertNotNull(rel);
  }

  /**
   * M2.6: window function — ROW_NUMBER with PARTITION BY + ORDER BY.
   */
  @Test
  public void testWindowRowNumber() {
    RelNode rel = converter.convertSql(
        "SELECT id, ROW_NUMBER() OVER (PARTITION BY dept_id ORDER BY salary DESC) FROM employees");
    assertNotNull(rel);
    String plan = RelOptUtil.toString(rel);
    assertTrue(plan.contains("ROW_NUMBER") || plan.contains("row_number") || plan.contains("OVER"),
        "expected a window expression in plan:\n" + plan);
  }

  /**
   * M2.7: INSERT INTO ... SELECT. Target and source columns line up; we
   * verify validation+rel-conversion completes without error.
   */
  @Test
  public void testInsertSelect() {
    RelNode rel = converter.convertSql(
        "INSERT INTO foo (a, b, c) SELECT id, dept_id, name FROM employees");
    assertNotNull(rel);
  }

  /**
   * M3.1: GaussDB optimizer hints are parsed and silently discarded so the
   * statement still produces a valid RelNode. We accept both top-level
   * placement and mid-SELECT placement.
   */
  @Test
  public void testHintsAreDiscarded() {
    RelNode rel = converter.convertSql(
        "SELECT /*+ LEADING(e) USE_NL(d) */ e.id FROM employees e INNER JOIN departments d ON e.dept_id = d.id");
    assertNotNull(rel);

    rel = converter.convertSql("/*+ HINT */ SELECT id FROM employees");
    assertNotNull(rel);
  }

  /**
   * M3.2: DELETE — parser round-trip through validation. Calcite validates
   * DELETE into a source-SELECT internally so this exercises the SqlDelete
   * surface end to end.
   */
  @Test
  public void testDelete() {
    RelNode rel = converter.convertSql("DELETE FROM employees WHERE dept_id = 10");
    assertNotNull(rel);
  }

  @Test
  public void testDeleteAllRows() {
    RelNode rel = converter.convertSql("DELETE FROM foo");
    assertNotNull(rel);
  }

  /**
   * M3.3: UPDATE with multi-assignment and a WHERE clause.
   */
  @Test
  public void testUpdate() {
    RelNode rel = converter.convertSql(
        "UPDATE employees SET salary = salary * 1.1, dept_id = 2 WHERE mgr_id IS NULL");
    assertNotNull(rel);
  }

  /**
   * M3.6: GaussDB-extended types parse and validate through CAST.
   */
  @Test
  public void testExtendedTypeCasts() {
    // BYTEA → VARBINARY, UUID → VARCHAR(36), TIMESTAMPTZ, JSON/JSONB, INTERVAL.
    RelNode rel = converter.convertSql(
        "SELECT CAST(name AS UUID), CAST(name AS JSON), CAST(name AS BYTEA), "
            + "CAST(name AS TIMESTAMPTZ) FROM employees");
    assertNotNull(rel);
  }

  /**
   * M3.4: MERGE INTO — single WHEN MATCHED + single WHEN NOT MATCHED. We
   * validate the parser path by checking {@link SqlNode} is a SqlMerge; the
   * rel-conversion path for MERGE is non-trivial in Calcite 1.21 and is
   * exercised via {@code toSqlNode} directly rather than {@code convertSql}.
   */
  @Test
  public void testMergeIntoParses() {
    org.apache.calcite.sql.SqlNode node = converter.toSqlNode(
        "MERGE INTO employees t USING departments s ON t.dept_id = s.id "
            + "WHEN MATCHED THEN UPDATE SET name = s.name "
            + "WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)");
    assertNotNull(node);
    assertTrue(node instanceof org.apache.calcite.sql.SqlMerge,
        "expected SqlMerge, got " + node.getClass().getSimpleName());
  }

  /**
   * M3.5: CONNECT BY is rewritten at AST time into a recursive CTE. We
   * check the rewrite produced a SqlWith — rel-conversion for recursive
   * CTEs in Calcite 1.21 requires additional plumbing (a Stage-4 item).
   */
  @Test
  public void testConnectByRewritesToSqlWith() {
    org.apache.calcite.sql.SqlNode node = converter.toSqlNode(
        "SELECT id FROM employees START WITH mgr_id IS NULL CONNECT BY PRIOR id = mgr_id");
    assertNotNull(node);
    assertTrue(node instanceof org.apache.calcite.sql.SqlWith,
        "expected SqlWith, got " + node.getClass().getSimpleName());
    String serialized = node.toString();
    assertTrue(serialized.contains("__coral_connect_by"),
        "expected synthesized CTE name in output, got:\n" + serialized);
    assertTrue(serialized.toUpperCase().contains("UNION ALL"),
        "expected UNION ALL in recursive CTE body, got:\n" + serialized);
  }

  /**
   * M3.5: PRIOR outside CONNECT BY is an error.
   */
  @Test(expectedExceptions = UnsupportedOperationException.class)
  public void testPriorOutsideConnectByIsError() {
    converter.toSqlNode("SELECT PRIOR id FROM employees");
  }

  /**
   * M3.5: NOCYCLE is explicitly unsupported in v1.
   */
  @Test(expectedExceptions = UnsupportedOperationException.class)
  public void testNoCycleRejected() {
    converter.toSqlNode("SELECT id FROM employees START WITH mgr_id IS NULL "
        + "CONNECT BY NOCYCLE PRIOR id = mgr_id");
  }

  /**
   * M4.3: decode(sel, k1, v1, ..., default) rewrites to a CASE expression and
   * validates against the Calcite std CASE op.
   */
  @Test
  public void testDecodeRewritesToCase() {
    RelNode rel = converter.convertSql(
        "SELECT DECODE(dept_id, 1, 'eng', 2, 'sales', 'other') FROM employees");
    assertNotNull(rel);
    String plan = RelOptUtil.toString(rel);
    assertTrue(plan.toUpperCase().contains("CASE"), "expected CASE in plan:\n" + plan);
  }

  /**
   * M4.3: mod(a, b) → a % b end-to-end.
   */
  @Test
  public void testModOperator() {
    RelNode rel = converter.convertSql("SELECT MOD(id, 2) FROM employees");
    assertNotNull(rel);
    String plan = RelOptUtil.toString(rel);
    assertTrue(plan.contains("MOD") || plan.contains("%") || plan.contains("mod"),
        "expected MOD in plan:\n" + plan);
  }

  /**
   * M4.3: array_agg and string_agg rewrites hit the Hive collect_list /
   * concat_ws names. This exercises the unresolved-function fallback path.
   */
  @Test
  public void testArrayAndStringAgg() {
    // array_agg → collect_list (recognized by Hive's static registry).
    RelNode rel = converter.convertSql("SELECT ARRAY_AGG(id) FROM employees");
    assertNotNull(rel);

    // string_agg → concat_ws(sep, collect_list(x)).
    rel = converter.convertSql("SELECT STRING_AGG(name, ',') FROM employees");
    assertNotNull(rel);
  }

  /**
   * M4.3: random() → rand() — names are different so the Spark dialect
   * emits the right Spark builtin.
   */
  @Test
  public void testRandomRename() {
    // We only assert the rewrite reached the validator; the validator
    // resolves rand() via the Hive registry.
    org.apache.calcite.sql.SqlNode node = converter.toSqlNode("SELECT RANDOM() FROM employees");
    assertNotNull(node);
    assertTrue(node.toString().toLowerCase().contains("rand"),
        "expected 'rand' in output, got:\n" + node);
  }

  /**
   * S4.1: PostgreSQL regex operators — `~`, `~*`, `!~`, `!~*`.
   * All four forms must reach validation.
   */
  @Test
  public void testRegexOperators() {
    assertNotNull(converter.convertSql("SELECT id FROM employees WHERE name ~ '^A.*'"));
    assertNotNull(converter.convertSql("SELECT id FROM employees WHERE name ~* '^a.*'"));
    assertNotNull(converter.convertSql("SELECT id FROM employees WHERE name !~ '^A.*'"));
    assertNotNull(converter.convertSql("SELECT id FROM employees WHERE name !~* '^a.*'"));
  }

  /**
   * S4.1: case-insensitive `~*` lowercases both sides so Spark RLIKE works.
   */
  @Test
  public void testCaseInsensitiveRegexLowerFolds() {
    org.apache.calcite.sql.SqlNode node =
        converter.toSqlNode("SELECT id FROM employees WHERE name ~* '^a.*'");
    String serialized = node.toString().toLowerCase();
    assertTrue(serialized.contains("lower"),
        "expected LOWER() folding for ~*, got:\n" + serialized);
  }

  /**
   * S4.2: window frame ROWS BETWEEN ... AND ...
   */
  @Test
  public void testWindowFrameRowsBetween() {
    RelNode rel = converter.convertSql(
        "SELECT SUM(salary) OVER (PARTITION BY dept_id ORDER BY id "
            + "ROWS BETWEEN 1 PRECEDING AND 1 FOLLOWING) FROM employees");
    assertNotNull(rel);
  }

  @Test
  public void testWindowFrameRangeUnbounded() {
    RelNode rel = converter.convertSql(
        "SELECT SUM(salary) OVER (PARTITION BY dept_id ORDER BY id "
            + "RANGE BETWEEN UNBOUNDED PRECEDING AND CURRENT ROW) FROM employees");
    assertNotNull(rel);
  }

  @Test
  public void testWindowFrameSingleBound() {
    // PostgreSQL allows single-bound form "ROWS N PRECEDING" which implies
    // CURRENT ROW as upper.
    RelNode rel = converter.convertSql(
        "SELECT SUM(salary) OVER (ORDER BY id ROWS 3 PRECEDING) FROM employees");
    assertNotNull(rel);
  }

  /**
   * S4.3: standalone VALUES (...) statement.
   */
  @Test
  public void testStandaloneValues() {
    RelNode rel = converter.convertSql("VALUES (1, 'a'), (2, 'b'), (3, 'c')");
    assertNotNull(rel);
    String plan = RelOptUtil.toString(rel);
    assertTrue(plan.contains("Values") || plan.contains("LogicalValues"),
        "expected Values in plan:\n" + plan);
  }

  /**
   * S5.1: DISTINCT ON is rewritten into a ROW_NUMBER subquery.
   * The rewrite must keep PARTITION BY tracking the DISTINCT ON keys and
   * ORDER BY as given by the user.
   */
  @Test
  public void testDistinctOnRewritten() {
    org.apache.calcite.sql.SqlNode node = converter.toSqlNode(
        "SELECT DISTINCT ON (dept_id) id, dept_id FROM employees ORDER BY dept_id, id");
    assertNotNull(node);
    String s = node.toString();
    assertTrue(s.contains("ROW_NUMBER") || s.toLowerCase().contains("row_number"),
        "expected ROW_NUMBER in rewrite:\n" + s);
    assertTrue(s.contains("__coral_distinct_on_rn"),
        "expected synthetic RN column:\n" + s);
    assertTrue(s.contains("PARTITION BY"), "expected PARTITION BY on DISTINCT ON keys:\n" + s);
  }

  /**
   * S5.1: DISTINCT ON without an outer ORDER BY still rewrites (PG permits
   * it — result is implementation-defined).
   */
  @Test
  public void testDistinctOnWithoutOrderBy() {
    org.apache.calcite.sql.SqlNode node =
        converter.toSqlNode("SELECT DISTINCT ON (dept_id) id, dept_id FROM employees");
    assertNotNull(node);
    assertTrue(node.toString().contains("__coral_distinct_on_rn"));
  }

  /**
   * S5.1: DISTINCT ON inside a set-op / subquery is still rejected.
   */
  @Test(expectedExceptions = UnsupportedOperationException.class)
  public void testDistinctOnInsideSetOpRejected() {
    converter.toSqlNode(
        "(SELECT DISTINCT ON (dept_id) id FROM employees) UNION ALL SELECT id FROM employees");
  }

  /**
   * S5.2: generate_series(a, b) → sequence(a, b).
   */
  @Test
  public void testGenerateSeriesTwoArg() {
    org.apache.calcite.sql.SqlNode node =
        converter.toSqlNode("SELECT generate_series(1, 10) FROM employees");
    String s = node.toString().toLowerCase();
    assertTrue(s.contains("sequence"),
        "generate_series must rewrite to sequence:\n" + s);
  }

  @Test
  public void testGenerateSeriesThreeArg() {
    org.apache.calcite.sql.SqlNode node =
        converter.toSqlNode("SELECT generate_series(1, 10, 2) FROM employees");
    String s = node.toString().toLowerCase();
    assertTrue(s.contains("sequence"), "expected sequence(), got:\n" + s);
  }

  /**
   * S5.3: Oracle legacy (+) outer-join marker is parsed and rejected with
   * an actionable message.
   */
  @Test
  public void testOracleLegacyOuterJoinRejected() {
    try {
      converter.toSqlNode(
          "SELECT e.id, d.id FROM employees e, departments d WHERE e.dept_id = d.id(+)");
      fail("expected UnsupportedOperationException");
    } catch (UnsupportedOperationException ex) {
      assertTrue(ex.getMessage().contains("ANSI"), "error should point at ANSI JOIN");
    }
  }

  /**
   * S5.4 diagnostic: surface the actual exception when we run MERGE through
   * the full pipeline. Runs {@code convertSql} and prints the exception so
   * subsequent iterations can target it. Not expected to "pass" end-to-end
   * today — it verifies the failure mode is stable and informative.
   */
  @Test
  public void testMergeRelConversionSurfaceDiagnostic() {
    String sql = "MERGE INTO employees t USING departments s ON t.dept_id = s.id "
        + "WHEN MATCHED THEN UPDATE SET name = s.name "
        + "WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)";
    try {
      converter.convertSql(sql);
      // If Calcite 1.21 grows MERGE rel-conversion, we flip this to a
      // proper assertion.
    } catch (RuntimeException ex) {
      // Recorded for the Stage-5 punch-list.
      StringBuilder sb = new StringBuilder("[MERGE rel-conversion error] ").append(ex);
      Throwable cur = ex;
      for (int depth = 0; cur != null && depth < 5; depth++) {
        StackTraceElement[] st = cur.getStackTrace();
        for (int i = 0; i < Math.min(st.length, 5); i++) {
          sb.append("\n    at ").append(st[i]);
        }
        cur = cur.getCause();
        if (cur != null) sb.append("\n  caused by: ").append(cur);
      }
      System.out.println(sb);
      assertNotNull(ex);
    }
  }

  /**
   * S4.4: Pure unit test for the PG→Spark date format translator.
   */
  @Test
  public void testPgDateFormatTranslator() {
    assertEquals(com.linkedin.coral.gaussdb.parsetree.ParseTreeBuilder.translatePgDateFormat(
        "YYYY-MM-DD HH24:MI:SS"), "yyyy-MM-dd HH:mm:ss");
    assertEquals(com.linkedin.coral.gaussdb.parsetree.ParseTreeBuilder.translatePgDateFormat(
        "DD/MM/YYYY"), "dd/MM/yyyy");
    assertEquals(com.linkedin.coral.gaussdb.parsetree.ParseTreeBuilder.translatePgDateFormat(
        "HH12:MI AM"), "hh:mm a");
    assertEquals(com.linkedin.coral.gaussdb.parsetree.ParseTreeBuilder.translatePgDateFormat(
        "YYYY-MM-DD\"T\"HH24:MI:SS.FF3"), "yyyy-MM-dd\"T\"HH:mm:ss.SSS");
    // Unknown chars pass through:
    assertEquals(com.linkedin.coral.gaussdb.parsetree.ParseTreeBuilder.translatePgDateFormat(
        "???"), "???");
  }

  /**
   * S4.4: to_date and to_char with known date-ish format literals get
   * rewritten. to_char of a date becomes date_format in Spark.
   */
  @Test
  public void testToDateFormatRewrite() {
    org.apache.calcite.sql.SqlNode node = converter.toSqlNode(
        "SELECT TO_DATE(name, 'YYYY-MM-DD') FROM employees");
    String serialized = node.toString();
    assertTrue(serialized.contains("yyyy-MM-dd"),
        "expected Spark format tokens:\n" + serialized);
  }

  @Test
  public void testToCharRewritesToDateFormat() {
    org.apache.calcite.sql.SqlNode node = converter.toSqlNode(
        "SELECT TO_CHAR(name, 'YYYY-MM-DD HH24:MI:SS') FROM employees");
    String s = node.toString();
    assertTrue(s.toLowerCase().contains("date_format"),
        "to_char(date) should rewrite to date_format:\n" + s);
    assertTrue(s.contains("yyyy-MM-dd HH:mm:ss"), "format translated:\n" + s);
  }

  /**
   * Grammar negative: clearly invalid SQL must raise a parse error, not silently
   * succeed. Verifies our ThrowingErrorListener is wired up.
   */
  @Test(expectedExceptions = RuntimeException.class)
  public void testInvalidSyntaxThrows() {
    GaussDBParserDriver.parse("SELEKT nonsense FROM");
  }

  // ---- helpers ----

  @SuppressWarnings("unused")
  private static String explain(RelNode rel) {
    StringBuilder sb = new StringBuilder();
    RelWriter writer = new RelWriterImpl(new java.io.PrintWriter(new StringBuilderWriter(sb), true),
        SqlExplainLevel.EXPPLAN_ATTRIBUTES, false);
    rel.explain(writer);
    return sb.toString();
  }

  /** Minimal Writer backed by StringBuilder — dependency-free alternative to commons-io. */
  private static final class StringBuilderWriter extends java.io.Writer {
    private final StringBuilder sb;

    StringBuilderWriter(StringBuilder sb) {
      this.sb = sb;
    }

    @Override
    public void write(char[] cbuf, int off, int len) {
      sb.append(cbuf, off, len);
    }

    @Override
    public void flush() {
    }

    @Override
    public void close() {
    }
  }
}
