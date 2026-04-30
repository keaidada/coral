/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb;

import java.util.List;

import javax.annotation.Nonnull;

import com.google.common.base.Preconditions;

import org.apache.calcite.plan.RelOptTable;
import org.apache.calcite.rel.RelRoot;
import org.apache.calcite.rel.type.RelDataType;
import org.apache.calcite.sql.SqlNode;
import org.apache.calcite.util.Util;


/**
 * Expands views during GaussDB-to-Rel conversion. In v1 we do not expect
 * GaussDB workloads to consume Hive views, but the {@link RelOptTable.ViewExpander}
 * contract must be provided to the Calcite {@code SqlToRelConverter}. Mirrors
 * {@code TrinoViewExpander}.
 */
public class GaussDBViewExpander implements RelOptTable.ViewExpander {

  private final GaussDBToRelConverter converter;

  public GaussDBViewExpander(@Nonnull GaussDBToRelConverter converter) {
    this.converter = converter;
  }

  @Override
  public RelRoot expandView(RelDataType rowType, String queryString, List<String> schemaPath, List<String> viewPath) {
    Preconditions.checkNotNull(viewPath);
    Preconditions.checkState(!viewPath.isEmpty());
    String dbName = Util.last(schemaPath);
    String tableName = viewPath.get(0);
    SqlNode sqlNode = converter.processView(dbName, tableName);
    return converter.getSqlToRelConverter().convertQuery(sqlNode, true, true);
  }
}
