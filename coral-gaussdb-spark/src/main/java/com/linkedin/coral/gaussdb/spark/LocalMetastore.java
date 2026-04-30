/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb.spark;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.Map;

import org.apache.hadoop.hive.metastore.api.Database;
import org.apache.hadoop.hive.metastore.api.FieldSchema;
import org.apache.hadoop.hive.metastore.api.StorageDescriptor;
import org.apache.hadoop.hive.metastore.api.Table;

import com.linkedin.coral.common.HiveMetastoreClient;


/**
 * In-memory {@link HiveMetastoreClient} backed by the same {@code Map<db, Map<table,
 * List<"col|type">>>} structure that {@link com.linkedin.coral.common.LocalMetastoreHiveSchema}
 * consumes. Lets tests (and simple command-line usage) translate GaussDB → Spark
 * without needing a real Hive metastore.
 *
 * <p>Behavior is intentionally minimal: unsupported calls return empty results
 * or {@code null} rather than throwing, mirroring the philosophy of
 * {@code LocalMetastoreHiveSchema}. The tables returned are plain Hive tables
 * (not VIRTUAL_VIEW) so {@code CoralSpark.create} sees them as leaf base tables.
 *
 * <p>This is a lightweight convenience for GaussDB users without Hive infra.
 * In production with a real metastore, pass your own {@code HiveMetastoreClient}
 * to {@link CoralGaussDBToSpark#create(String, HiveMetastoreClient)} directly.
 */
public final class LocalMetastore implements HiveMetastoreClient {

  /** Keys are lowercase db names. */
  private final Map<String, Map<String, List<String>>> catalog;

  public LocalMetastore(Map<String, Map<String, List<String>>> catalog) {
    this.catalog = catalog;
  }

  @Override
  public List<String> getAllDatabases() {
    return new ArrayList<>(catalog.keySet());
  }

  @Override
  public Database getDatabase(String dbName) {
    if (!catalog.containsKey(dbName)) {
      return null;
    }
    Database db = new Database();
    db.setName(dbName);
    db.setDescription("in-memory database");
    return db;
  }

  @Override
  public List<String> getAllTables(String dbName) {
    Map<String, List<String>> tables = catalog.get(dbName);
    return tables == null ? Collections.emptyList() : new ArrayList<>(tables.keySet());
  }

  @Override
  public Table getTable(String dbName, String tableName) {
    Map<String, List<String>> tables = catalog.get(dbName);
    if (tables == null) {
      return null;
    }
    List<String> cols = tables.get(tableName);
    if (cols == null) {
      return null;
    }
    Table t = new Table();
    t.setDbName(dbName);
    t.setTableName(tableName);
    t.setTableType("MANAGED_TABLE");
    StorageDescriptor sd = new StorageDescriptor();
    List<FieldSchema> fields = new ArrayList<>(cols.size());
    for (String col : cols) {
      String[] parts = col.split("\\|");
      String name = parts[0];
      String type = parts.length > 1 ? parts[1] : "string";
      fields.add(new FieldSchema(name, type, null));
    }
    sd.setCols(fields);
    // Empty SerDe / InputFormat — CoralSpark does not require them for a
    // base-table reference; when it does we can add a default here.
    t.setSd(sd);
    t.setPartitionKeys(Arrays.asList());
    return t;
  }
}
