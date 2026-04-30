/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb.spark;

import java.util.Collections;
import java.util.List;
import java.util.Map;

import javax.annotation.Nonnull;
import javax.annotation.Nullable;

import org.apache.calcite.rel.RelNode;
import org.apache.calcite.sql.SqlCall;
import org.apache.calcite.sql.SqlDialect;
import org.apache.calcite.sql.SqlIdentifier;
import org.apache.calcite.sql.SqlMerge;
import org.apache.calcite.sql.SqlNode;
import org.apache.calcite.sql.SqlNodeList;
import org.apache.calcite.sql.SqlWith;
import org.apache.calcite.sql.SqlWithItem;
import org.apache.calcite.sql.SqlWriter;
import org.apache.calcite.sql.pretty.SqlPrettyWriter;
import org.apache.calcite.util.Util;

import com.linkedin.coral.common.HiveMetastoreClient;
import com.linkedin.coral.gaussdb.GaussDBToRelConverter;
import com.linkedin.coral.spark.CoralSpark;
import com.linkedin.coral.spark.containers.SparkUDFInfo;
import com.linkedin.coral.spark.dialect.SparkSqlDialect;


/**
 * End-to-end convenience wrapper that translates a GaussDB / openGauss SQL string
 * into Spark SQL in one step.
 *
 * <p>Under the hood:
 * <pre>
 *   GaussDBToRelConverter.convertSql(sql) → RelNode → CoralSpark.create(rel, hms) → Spark SQL
 * </pre>
 *
 * <p>Typical usage:
 * <pre>{@code
 *   String sparkSql = CoralGaussDBToSpark.translate(sql, hms);
 * }</pre>
 *
 * <p>For statements that Calcite 1.21's {@code SqlToRelConverter} cannot yet
 * handle (e.g. {@code MERGE INTO}, recursive CTEs produced from
 * {@code CONNECT BY}), this class falls back to an AST-level unparse through
 * {@link SparkSqlDialect}. The result is still valid Spark SQL, just without
 * the benefit of RelNode-level optimization.
 *
 * <p>For callers without a Hive Metastore, an in-memory catalog map is accepted:
 * {@code Map<String, Map<String, List<String>>>} where the outer key is the database
 * name, the middle key is the table name, and the inner list is column names. Note:
 * the {@code CoralSpark} backend still requires a {@link HiveMetastoreClient} for
 * base-table resolution / UDF extraction. The local-metastore convenience ctor is
 * a Stage-2 item in the plan — for v1, prefer the HMS-backed overload.
 */
public final class CoralGaussDBToSpark {

  private final RelNode relNode;
  private final CoralSpark coralSpark;
  /** Populated when the statement used the SqlNode-level fallback path. */
  @Nullable
  private final String directSparkSql;

  private CoralGaussDBToSpark(RelNode relNode, CoralSpark coralSpark) {
    this.relNode = relNode;
    this.coralSpark = coralSpark;
    this.directSparkSql = null;
  }

  private CoralGaussDBToSpark(String directSparkSql) {
    this.relNode = null;
    this.coralSpark = null;
    this.directSparkSql = directSparkSql;
  }

  /**
   * HMS-backed entry point. Recommended for production.
   */
  public static CoralGaussDBToSpark create(@Nonnull String gaussdbSql, @Nonnull HiveMetastoreClient hms) {
    return create(gaussdbSql, hms, null);
  }

  /**
   * HMS-backed entry point with a {@link CoralGaussDBToSparkConfig}.
   *
   * <p>The config is reserved for future knobs (e.g. defaultDatabase for collapsing
   * GaussDB 3-part paths, function-mapping overrides). In v1 it is accepted but
   * does not yet influence behavior — this keeps the public API stable as Stage 2/3
   * land.
   */
  public static CoralGaussDBToSpark create(@Nonnull String gaussdbSql, @Nonnull HiveMetastoreClient hms,
      @Nullable CoralGaussDBToSparkConfig config) {
    GaussDBToRelConverter converter = new GaussDBToRelConverter(hms);
    SqlNode ast = converter.toSqlNode(gaussdbSql);
    if (needsDirectUnparse(ast)) {
      return new CoralGaussDBToSpark(unparseForSpark(ast));
    }
    RelNode rel = converter.convertSql(gaussdbSql);
    CoralSpark spark = CoralSpark.create(rel, hms);
    return new CoralGaussDBToSpark(rel, spark);
  }

  /**
   * One-liner that returns the Spark SQL string directly. Use when you don't need
   * the intermediate RelNode / base tables / UDF info.
   */
  public static String translate(@Nonnull String gaussdbSql, @Nonnull HiveMetastoreClient hms) {
    return create(gaussdbSql, hms).getSparkSql();
  }

  /**
   * Convenience overload backed by an in-memory catalog. For production
   * deployments always pass a real {@link HiveMetastoreClient}; this path is
   * intended for tests, CLI dry-runs, and users exploring GaussDB → Spark
   * translation without Hive infrastructure.
   *
   * @param gaussdbSql the source SQL in GaussDB dialect
   * @param catalog shape: {@code {dbName: {tableName: ["col|hiveType", ...]}}}.
   *     Hive types are parsed by {@code TypeInfoUtils.getTypeInfoFromTypeString}
   *     — typical values: {@code int}, {@code string}, {@code double},
   *     {@code timestamp}, {@code decimal(10,2)}, {@code array<string>}.
   */
  public static CoralGaussDBToSpark createLocal(@Nonnull String gaussdbSql,
      @Nonnull Map<String, Map<String, List<String>>> catalog) {
    LocalMetastore hms = new LocalMetastore(catalog);
    GaussDBToRelConverter converter = new GaussDBToRelConverter(catalog);
    SqlNode ast = converter.toSqlNode(gaussdbSql);
    if (needsDirectUnparse(ast)) {
      return new CoralGaussDBToSpark(unparseForSpark(ast));
    }
    RelNode rel = converter.convertSql(gaussdbSql);
    CoralSpark spark = CoralSpark.create(rel, hms);
    return new CoralGaussDBToSpark(rel, spark);
  }

  /**
   * Parser / IR-only entry point backed by an in-memory catalog. Useful in tests
   * and for users without a Hive Metastore who want to inspect the RelNode directly.
   *
   * <p>{@code localMetaStore} shape: {@code {dbName: {tableName: [col1, col2, ...]}}}.
   * Column types default to string; richer typing is a Stage 2 item.
   */
  public static RelNode toRelNode(@Nonnull String gaussdbSql,
      @Nonnull Map<String, Map<String, List<String>>> localMetaStore) {
    return new GaussDBToRelConverter(localMetaStore).convertSql(gaussdbSql);
  }

  public String getSparkSql() {
    if (directSparkSql != null) {
      return directSparkSql;
    }
    return coralSpark.getSparkSql();
  }

  public RelNode getRelNode() {
    return relNode;
  }

  public List<String> getBaseTables() {
    return coralSpark != null ? coralSpark.getBaseTables() : Collections.emptyList();
  }

  public List<SparkUDFInfo> getSparkUDFInfoList() {
    return coralSpark != null ? coralSpark.getSparkUDFInfoList() : Collections.emptyList();
  }

  /* ------------------------------------------------------------------ */

  /**
   * Returns true when the AST requires the SqlNode-level unparse path.
   * Today that means MERGE (Calcite 1.21 {@code convertMerge} has
   * preconditions we cannot satisfy from a pure parser path) and
   * {@link SqlWith} (our CONNECT BY rewrite produces a self-referential
   * CTE that the stock rel-converter rejects).
   */
  private static boolean needsDirectUnparse(SqlNode ast) {
    return ast instanceof SqlMerge || ast instanceof SqlWith;
  }

  /**
   * Unparses an AST using Spark's dialect. Produces valid Spark SQL syntax —
   * backticks, Spark function spellings — without touching the rel layer.
   * Used as a fallback for statements that Calcite cannot rel-convert in 1.21.
   *
   * <p>Calcite 1.21's default unparse for {@link SqlWith} / {@link SqlWithItem}
   * does not wrap the CTE body in parentheses and does not emit {@code RECURSIVE},
   * so for {@code SqlWith} we render manually to produce spark-compatible output:
   * <pre>
   *   WITH [RECURSIVE] cte_name AS (body) [, ...]
   *   outer_select
   * </pre>
   */
  private static String unparseForSpark(SqlNode ast) {
    SqlDialect dialect = SparkSqlDialect.INSTANCE;
    SqlPrettyWriter writer = new SqlPrettyWriter(dialect);
    if (ast instanceof SqlWith) {
      unparseSqlWith(writer, (SqlWith) ast);
    } else {
      ast.unparse(writer, 0, 0);
    }
    return writer.toString();
  }

  /**
   * Manual unparse for {@link SqlWith} that (a) wraps each CTE body in parentheses
   * and (b) emits {@code WITH RECURSIVE} when any CTE self-references its own name.
   * Both behaviors are required for Spark SQL to accept the translation from
   * {@code CONNECT BY} (which always produces a {@code UNION ALL}-bodied recursive CTE).
   */
  private static void unparseSqlWith(SqlPrettyWriter writer, SqlWith with) {
    boolean recursive = isRecursiveWith(with);
    writer.keyword("WITH");
    if (recursive) {
      writer.keyword("RECURSIVE");
    }
    boolean first = true;
    for (SqlNode node : with.withList) {
      if (!first) {
        writer.sep(",");
      }
      first = false;
      SqlWithItem item = (SqlWithItem) node;
      item.name.unparse(writer, 0, 0);
      if (item.columnList != null) {
        item.columnList.unparse(writer, 0, 0);
      }
      writer.keyword("AS");
      final SqlWriter.Frame frame = writer.startList("(", ")");
      item.query.unparse(writer, 0, 0);
      writer.endList(frame);
    }
    with.body.unparse(writer, 0, 0);
  }

  /**
   * Returns {@code true} if any CTE in the {@code WITH} list references its own name
   * anywhere in its body. The detection is intentionally conservative — false
   * positives only add the {@code RECURSIVE} keyword which Spark tolerates; false
   * negatives would produce an invalid plan.
   */
  private static boolean isRecursiveWith(SqlWith with) {
    for (SqlNode node : with.withList) {
      if (!(node instanceof SqlWithItem)) {
        continue;
      }
      SqlWithItem item = (SqlWithItem) node;
      String cteName = Util.last(item.name.names);
      if (bodyReferences(item.query, cteName)) {
        return true;
      }
    }
    return false;
  }

  private static boolean bodyReferences(SqlNode node, String name) {
    if (node == null) {
      return false;
    }
    if (node instanceof SqlIdentifier) {
      SqlIdentifier id = (SqlIdentifier) node;
      return !id.names.isEmpty() && Util.last(id.names).equals(name);
    }
    if (node instanceof SqlNodeList) {
      for (SqlNode child : (SqlNodeList) node) {
        if (bodyReferences(child, name)) {
          return true;
        }
      }
      return false;
    }
    if (node instanceof SqlCall) {
      for (SqlNode child : ((SqlCall) node).getOperandList()) {
        if (bodyReferences(child, name)) {
          return true;
        }
      }
    }
    return false;
  }
}
