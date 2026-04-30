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


/**
 * CLI smoke — translate a representative GaussDB SQL to Spark SQL and print.
 *
 * <pre>
 *   ./gradlew :coral-gaussdb-spark:run
 * </pre>
 *
 * Uses the in-memory catalog so no Hive Metastore is required. Prints each
 * statement + the translated Spark SQL separated by a divider.
 */
public final class SmokeDemo {

  private SmokeDemo() {
  }

  public static void main(String[] args) {
    Map<String, Map<String, List<String>>> catalog = buildCatalog();

    String[] samples = {
        // 1) 综合查询：CTE + JOIN + GROUP BY + HAVING + 窗口 + CASE + NVL + ||
        "WITH active_emp AS (\n"
            + "  SELECT id, name, dept_id, salary, mgr_id FROM employees WHERE salary > 0\n"
            + ")\n"
            + "SELECT\n"
            + "  d.name || ' / ' || NVL(e.name, 'n/a') AS label,\n"
            + "  COUNT(*) AS headcount,\n"
            + "  SUM(e.salary) AS total_pay,\n"
            + "  CASE WHEN AVG(e.salary) > 100 THEN 'high' ELSE 'low' END AS tier,\n"
            + "  ROW_NUMBER() OVER (PARTITION BY d.id ORDER BY SUM(e.salary) DESC) AS rn\n"
            + "FROM active_emp e\n"
            + "LEFT JOIN departments d ON e.dept_id = d.id\n"
            + "GROUP BY d.id, d.name, e.name\n"
            + "HAVING COUNT(*) > 0\n"
            + "ORDER BY SUM(e.salary) DESC",

        // 2) PG :: cast + DECODE + SUBSTR + MOD
        "SELECT\n"
            + "  id::BIGINT AS id64,\n"
            + "  DECODE(dept_id, 1, 'eng', 2, 'sales', 'other') AS dept_label,\n"
            + "  SUBSTR(name, 1, 3) AS short_name,\n"
            + "  MOD(id, 10) AS bucket\n"
            + "FROM employees WHERE dept_id IN (1, 2, 3)",

        // 3) UNION ALL + 正则 + 子查询
        "SELECT id FROM employees WHERE name ~* '^a.*'\n"
            + "UNION ALL\n"
            + "SELECT id FROM employees WHERE dept_id IN (SELECT id FROM departments WHERE name ~ 'Eng')",

        // 4) MERGE INTO（SqlNode-level unparse 回退路径）
        "MERGE INTO employees t USING departments s ON t.dept_id = s.id\n"
            + "  WHEN MATCHED THEN UPDATE SET name = s.name\n"
            + "  WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)",

        // 5) CONNECT BY 层次查询（回退路径，输出为 WITH … UNION ALL …）
        "SELECT id, name FROM employees\n"
            + "  START WITH mgr_id IS NULL\n"
            + "  CONNECT BY PRIOR id = mgr_id",

        // 6) DISTINCT ON 重写（输出含 ROW_NUMBER 子查询包装）
        "SELECT DISTINCT ON (dept_id) id, dept_id, salary\n"
            + "FROM employees ORDER BY dept_id, salary DESC",
    };

    int i = 1;
    for (String sql : samples) {
      System.out.println();
      System.out.println("=============== sample #" + i + " ===============");
      System.out.println("[GaussDB]");
      System.out.println(indent(sql));
      try {
        String spark = CoralGaussDBToSpark.createLocal(sql, catalog).getSparkSql();
        System.out.println("[Spark]");
        System.out.println(indent(spark));
      } catch (Exception ex) {
        System.out.println("[ERROR] " + ex);
      }
      i++;
    }
    System.out.println();
    System.out.println("=============== done ===============");
  }

  private static Map<String, Map<String, List<String>>> buildCatalog() {
    Map<String, Map<String, List<String>>> catalog = new HashMap<>();
    Map<String, List<String>> defaultSchema = new HashMap<>();
    defaultSchema.put("employees",
        Arrays.asList("id|int", "mgr_id|int", "name|string", "dept_id|int", "salary|double"));
    defaultSchema.put("departments", Arrays.asList("id|int", "name|string"));
    defaultSchema.put("foo", Arrays.asList("a|int", "b|int", "c|string"));
    catalog.put("default", defaultSchema);
    return catalog;
  }

  private static String indent(String s) {
    StringBuilder sb = new StringBuilder();
    for (String line : s.split("\n")) {
      sb.append("  ").append(line).append('\n');
    }
    return sb.toString();
  }
}
