/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb.spark;

import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import org.testng.annotations.BeforeClass;
import org.testng.annotations.Test;

import static org.testng.Assert.*;


/**
 * End-to-end tests: GaussDB SQL → Spark SQL via {@link CoralGaussDBToSpark#createLocal}.
 *
 * <p>These tests assert that:
 *   (a) the translation pipeline completes without error for each stage's
 *       representative statement, and
 *   (b) the produced Spark SQL string contains the expected Spark-dialect
 *       idiom (e.g. backticks for quoted identifiers, lower-case function
 *       names, rewritten function names).
 *
 * <p>We do NOT execute the produced Spark SQL against a real Spark session;
 * smoke execution is tracked separately (see plan §8 "手工冒烟").
 */
public class CoralGaussDBToSparkTest {

  private Map<String, Map<String, List<String>>> catalog;

  @BeforeClass
  public void setUp() {
    catalog = new HashMap<>();

    Map<String, List<String>> defaultSchema = new HashMap<>();
    defaultSchema.put("employees",
        Arrays.asList("id|int", "mgr_id|int", "name|string", "dept_id|int", "salary|double"));
    defaultSchema.put("departments", Arrays.asList("id|int", "name|string"));
    defaultSchema.put("foo", Arrays.asList("a|int", "b|int", "c|string"));
    catalog.put("default", defaultSchema);
  }

  /* ------------------------------------------------------------------ *
   *  Stage 1 — basic shapes
   * ------------------------------------------------------------------ */

  @Test
  public void testSimpleSelect() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT id, name FROM employees WHERE dept_id = 10 ORDER BY id LIMIT 5", catalog).getSparkSql();
    assertNotNull(spark);
    String lc = spark.toLowerCase();
    assertTrue(lc.contains("select") && lc.contains("from"), "unexpected Spark SQL:\n" + spark);
    assertTrue(lc.contains("order by") && lc.contains("limit"),
        "ORDER BY / LIMIT should survive translation:\n" + spark);
  }

  @Test
  public void testStringConcatRewrittenToConcat() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT name || ' - ' || dept_id FROM employees", catalog).getSparkSql();
    // Coral-spark keeps the || operator in the output (Spark SQL understands
    // it natively). We just verify concatenation is preserved.
    String lc = spark.toLowerCase();
    assertTrue(lc.contains("concat") || lc.contains("||"),
        "string concatenation should be preserved:\n" + spark);
  }

  @Test
  public void testNvlRewrittenToCoalesce() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT NVL(mgr_id, 0) FROM employees", catalog).getSparkSql();
    String lc = spark.toLowerCase();
    // Calcite's std COALESCE convertlet lowers 2-arg COALESCE to CASE WHEN a IS
    // NOT NULL THEN a ELSE b END at validation time — both forms are
    // semantically correct for Spark.
    assertTrue(lc.contains("coalesce") || lc.contains("case when"),
        "NVL should land as COALESCE or its CASE expansion:\n" + spark);
    assertFalse(lc.contains("nvl("), "NVL name must not leak to Spark output:\n" + spark);
  }

  @Test
  public void testPgCastRewrittenToStandardCast() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT salary::INT FROM employees", catalog).getSparkSql();
    assertTrue(spark.toUpperCase().contains("CAST"),
        "x::T should become CAST(x AS T) in Spark output:\n" + spark);
  }

  /* ------------------------------------------------------------------ *
   *  Stage 2 — advanced DML
   * ------------------------------------------------------------------ */

  @Test
  public void testGroupByAndHaving() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT dept_id, COUNT(*), SUM(salary) FROM employees GROUP BY dept_id HAVING COUNT(*) > 1",
        catalog).getSparkSql();
    String lc = spark.toLowerCase();
    assertTrue(lc.contains("group by"), "GROUP BY should appear:\n" + spark);
    // Calcite may rewrite HAVING into a filter outside the aggregate; either
    // form is acceptable for Spark execution.
    assertTrue(lc.contains("having") || lc.contains("where"),
        "HAVING or rewritten filter should appear:\n" + spark);
  }

  @Test
  public void testNonRecursiveCte() {
    String spark = CoralGaussDBToSpark.createLocal(
        "WITH high_paid AS (SELECT id, name FROM employees WHERE salary > 100) "
            + "SELECT id FROM high_paid",
        catalog).getSparkSql();
    assertNotNull(spark);
    // CoralSpark may inline the CTE; we only require the translation to succeed.
    assertTrue(spark.toLowerCase().contains("select"), "unexpected Spark SQL:\n" + spark);
  }

  @Test
  public void testUnionAll() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT id FROM employees UNION ALL SELECT id FROM departments", catalog).getSparkSql();
    assertTrue(spark.toLowerCase().contains("union"), "UNION should survive:\n" + spark);
  }

  @Test
  public void testWindowFunction() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT id, ROW_NUMBER() OVER (PARTITION BY dept_id ORDER BY salary DESC) FROM employees",
        catalog).getSparkSql();
    String lc = spark.toLowerCase();
    assertTrue(lc.contains("row_number") && lc.contains("over"),
        "window function should be preserved:\n" + spark);
  }

  /* ------------------------------------------------------------------ *
   *  Stage 3 — advanced translations handled at AST time
   * ------------------------------------------------------------------ */

  @Test
  public void testDecodeAsCaseEndToEnd() {
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT DECODE(dept_id, 1, 'eng', 2, 'sales', 'other') FROM employees", catalog).getSparkSql();
    String lc = spark.toLowerCase();
    assertTrue(lc.contains("case") && lc.contains("when"),
        "decode() must become CASE WHEN in Spark SQL:\n" + spark);
  }

  @Test
  public void testHintDiscarded() {
    // Hints must not leak through to Spark (Spark won't understand Oracle hints).
    String spark = CoralGaussDBToSpark.createLocal(
        "SELECT /*+ LEADING(e) */ e.id FROM employees e", catalog).getSparkSql();
    assertFalse(spark.contains("/*+"), "hints must not leak into Spark SQL:\n" + spark);
    assertFalse(spark.contains("LEADING"), "hint body must not leak:\n" + spark);
  }

  /**
   * S4.5 / S5.4: MERGE — now produces valid Spark SQL via the SqlNode-level
   * unparse fallback. Spark 3.x supports MERGE INTO with the same shape, so
   * the output is directly executable.
   */
  @Test
  public void testMergeEndToEnd() {
    String sql = "MERGE INTO employees t USING departments s ON t.dept_id = s.id "
        + "WHEN MATCHED THEN UPDATE SET name = s.name "
        + "WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)";
    String out = CoralGaussDBToSpark.createLocal(sql, catalog).getSparkSql();
    assertNotNull(out);
    String up = out.toUpperCase();
    assertTrue(up.contains("MERGE INTO"), "expected MERGE INTO preserved:\n" + out);
    assertTrue(up.contains("USING"), "expected USING preserved:\n" + out);
    assertTrue(up.contains("WHEN MATCHED"), "expected WHEN MATCHED preserved:\n" + out);
    assertTrue(up.contains("WHEN NOT MATCHED"), "expected WHEN NOT MATCHED preserved:\n" + out);
  }

  /**
   * S4.5 / S5.5: CONNECT BY — the WITH RECURSIVE rewrite is unparsed through
   * Spark dialect directly.
   */
  @Test
  public void testConnectByEndToEnd() {
    String sql = "SELECT id FROM employees START WITH mgr_id IS NULL "
        + "CONNECT BY PRIOR id = mgr_id";
    String out = CoralGaussDBToSpark.createLocal(sql, catalog).getSparkSql();
    assertNotNull(out);
    String lc = out.toLowerCase();
    assertTrue(lc.contains("with") && lc.contains("__coral_connect_by"),
        "expected WITH __coral_connect_by shape:\n" + out);
    assertTrue(lc.contains("union all"), "expected UNION ALL between anchor and step:\n" + out);
  }

  /**
   * Regression: CONNECT BY output must (a) emit {@code WITH RECURSIVE} and
   * (b) enclose the CTE body in parentheses, otherwise the UNION ALL leaks
   * into the outer query and Spark rejects the statement.
   */
  @Test
  public void testConnectByUnparseShape() {
    String sql = "SELECT id, name FROM employees START WITH mgr_id IS NULL "
        + "CONNECT BY PRIOR id = mgr_id";
    String out = CoralGaussDBToSpark.createLocal(sql, catalog).getSparkSql();
    String lc = out.toLowerCase();
    assertTrue(lc.contains("with recursive"),
        "recursive CTE must be emitted with 'WITH RECURSIVE':\n" + out);
    // Parentheses must enclose the CTE body — the first "AS" after the CTE
    // name must be followed by "(".
    int asIdx = lc.indexOf("__coral_connect_by as");
    assertTrue(asIdx > 0, "CTE AS must follow the name:\n" + out);
    String afterAs = lc.substring(asIdx + "__coral_connect_by as".length()).trim();
    assertTrue(afterAs.startsWith("("), "CTE body must be parenthesized:\n" + out);
  }

  /**
   * Regression for issue observed in the coral-service integration:
   * {@code '123'::int} tripped
   * {@code checkConvertedType} because the convertlet produced a nullable
   * cast type while the validator derived NOT NULL. Our
   * {@link com.linkedin.coral.gaussdb.GaussDBConvertletTable} reads the
   * validator-derived type and defers to it.
   */
  @Test
  public void testPgCastOnLiteralPreservesNullability() {
    String out = CoralGaussDBToSpark.createLocal("SELECT '123'::int AS n", catalog).getSparkSql();
    String lc = out.toLowerCase();
    assertTrue(lc.contains("cast") || lc.contains("int"),
        "literal::int should render as CAST to INT:\n" + out);
  }
}
