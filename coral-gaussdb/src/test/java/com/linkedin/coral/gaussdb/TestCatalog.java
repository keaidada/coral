/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb;

import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;


/**
 * Shared in-memory catalog fixture for coral-gaussdb tests. Mirrors the map
 * shape accepted by {@link com.linkedin.coral.common.LocalMetastoreHiveSchema}:
 * {@code {dbName: {tableName: [col1|type1, col2|type2, ...]}}}.
 *
 * <p>Each column entry is formatted as {@code "name|hiveType"} because
 * {@code LocalMetastoreHiveTable} parses the pipe-delimited string to build a
 * Calcite RelDataType. Types use the Hive type-string syntax
 * ({@code int}, {@code string}, {@code double}, ...).
 *
 * <p>Keeping the fixture small and explicit so individual tests can see at a
 * glance what tables / columns are available; if we outgrow this we can load
 * from a JSON resource.
 */
final class TestCatalog {

  private TestCatalog() {
  }

  static Map<String, Map<String, List<String>>> defaultCatalog() {
    Map<String, Map<String, List<String>>> catalog = new HashMap<>();

    Map<String, List<String>> publicSchema = new HashMap<>();
    publicSchema.put("employees",
        Arrays.asList("id|int", "mgr_id|int", "name|string", "dept_id|int", "salary|double"));
    publicSchema.put("departments", Arrays.asList("id|int", "name|string"));
    catalog.put("public", publicSchema);

    Map<String, List<String>> salesSchema = new HashMap<>();
    salesSchema.put("orders", Arrays.asList("id|int", "cust_id|int", "amount|double", "ts|timestamp"));
    catalog.put("sales", salesSchema);

    // Also expose a "default" db so unqualified table references resolve in
    // the test harness — the Coral catalog reader defaults to ["hive", "default"]
    // path, and we want to exercise unqualified lookups too.
    Map<String, List<String>> defaultSchema = new HashMap<>();
    defaultSchema.put("foo", Arrays.asList("a|int", "b|int", "c|string"));
    // Mirror the primary tables into "default" so existing test SQL that
    // writes `FROM employees` works too.
    defaultSchema.put("employees",
        Arrays.asList("id|int", "mgr_id|int", "name|string", "dept_id|int", "salary|double"));
    defaultSchema.put("departments", Arrays.asList("id|int", "name|string"));
    catalog.put("default", defaultSchema);

    return catalog;
  }
}
