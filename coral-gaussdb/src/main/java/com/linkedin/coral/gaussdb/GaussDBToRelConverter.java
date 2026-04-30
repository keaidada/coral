/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb;

import java.util.List;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;

import org.apache.calcite.adapter.java.JavaTypeFactory;
import org.apache.calcite.plan.RelOptCluster;
import org.apache.calcite.plan.volcano.VolcanoPlanner;
import org.apache.calcite.sql.SqlNode;
import org.apache.calcite.sql.SqlOperatorTable;
import org.apache.calcite.sql.fun.SqlStdOperatorTable;
import org.apache.calcite.sql.util.ChainedSqlOperatorTable;
import org.apache.calcite.sql.validate.SqlValidator;
import org.apache.calcite.sql2rel.SqlRexConvertletTable;
import org.apache.calcite.sql2rel.SqlToRelConverter;
import org.apache.hadoop.hive.metastore.api.Table;

import com.linkedin.coral.common.HiveMetastoreClient;
import com.linkedin.coral.common.HiveRelBuilder;
import com.linkedin.coral.common.ToRelConverter;
import com.linkedin.coral.gaussdb.parsetree.GaussDBParserDriver;
import com.linkedin.coral.gaussdb.parsetree.ParseTreeBuilder;
import com.linkedin.coral.hive.hive2rel.DaliOperatorTable;
import com.linkedin.coral.hive.hive2rel.HiveSqlValidator;
import com.linkedin.coral.hive.hive2rel.functions.HiveFunctionResolver;
import com.linkedin.coral.hive.hive2rel.functions.StaticHiveFunctionRegistry;

import static com.linkedin.coral.gaussdb.GaussDBSqlConformance.GAUSSDB_SQL;


/**
 * Entry point for converting GaussDB / openGauss SQL to Calcite {@link org.apache.calcite.rel.RelNode}.
 *
 * <p>Follows the same 3-layer pipeline as {@code TrinoToRelConverter}:
 * <pre>
 *   String SQL
 *     ─▶ ANTLR4 GaussDBSqlParser (GaussDBParserDriver)
 *       ─▶ ParseTreeBuilder visitor → Calcite SqlNode
 *         ─▶ HiveSqlValidator → validated SqlNode
 *           ─▶ SqlToRelConverter → RelNode
 * </pre>
 *
 * <p>For downstream Spark SQL emission, hand the RelNode to {@code CoralSpark.create(rel, ...)}.
 * Or use the convenience wrapper in {@code coral-gaussdb-spark}.
 *
 * <p>The GaussDB 3-part name {@code database.schema.table} is collapsed to Hive's
 * 2-part {@code db.table} by mapping {@code schema → db}; the top-level
 * {@code database} segment (if present) is ignored. This matches the user-approved
 * decision in the design plan.
 */
public class GaussDBToRelConverter extends ToRelConverter {

  private final ParseTreeBuilder parseTreeBuilder = new ParseTreeBuilder();
  private final HiveFunctionResolver functionResolver =
      new HiveFunctionResolver(new StaticHiveFunctionRegistry(), new ConcurrentHashMap<>());
  // The validator must be reused (stateful).
  private final SqlValidator sqlValidator = new HiveSqlValidator(getOperatorTable(), getCalciteCatalogReader(),
      (JavaTypeFactory) getRelBuilder().getTypeFactory(), GAUSSDB_SQL);

  public GaussDBToRelConverter(HiveMetastoreClient hiveMetastoreClient) {
    super(hiveMetastoreClient);
  }

  public GaussDBToRelConverter(Map<String, Map<String, List<String>>> localMetaStore) {
    super(localMetaStore);
  }

  @Override
  protected SqlRexConvertletTable getConvertletTable() {
    return new GaussDBConvertletTable();
  }

  @Override
  protected SqlValidator getSqlValidator() {
    return sqlValidator;
  }

  @Override
  protected SqlOperatorTable getOperatorTable() {
    // Chain Calcite standard operators + Dali/Hive UDF resolver. This gives us
    // SUM/COUNT/AVG/SUBSTRING/COALESCE/etc. for free and lets ParseTreeBuilder
    // reuse Hive's rich function registry without duplicating entries.
    return ChainedSqlOperatorTable.of(SqlStdOperatorTable.instance(), new DaliOperatorTable(functionResolver));
  }

  @Override
  protected SqlToRelConverter getSqlToRelConverter() {
    return new SqlToRelConverter(new GaussDBViewExpander(this), getSqlValidator(), getCalciteCatalogReader(),
        RelOptCluster.create(new VolcanoPlanner(), getRelBuilder().getRexBuilder()), getConvertletTable(),
        SqlToRelConverter.configBuilder().withRelBuilderFactory(HiveRelBuilder.LOGICAL_BUILDER).build());
  }

  @Override
  protected SqlNode toSqlNode(String sql, Table view) {
    return GaussDBParserDriver.parse(sql).accept(parseTreeBuilder);
  }
}
