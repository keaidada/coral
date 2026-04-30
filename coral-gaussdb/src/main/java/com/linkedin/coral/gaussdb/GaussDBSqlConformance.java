/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb;

import org.apache.calcite.sql.validate.SqlConformance;
import org.apache.calcite.sql.validate.SqlConformanceEnum;
import org.apache.calcite.sql.validate.SqlDelegatingConformance;


/**
 * SQL conformance settings used when validating GaussDB input.
 *
 * <p>Starts from {@link SqlConformanceEnum#PRAGMATIC_2003} — the same baseline as
 * {@code TrinoSqlConformance} — and overrides specific behaviors where GaussDB
 * diverges:
 *
 * <ul>
 *   <li>{@code !=} is accepted as an alias for {@code <>} (standard already in PRAGMATIC_2003 via the lexer).</li>
 *   <li>{@code MINUS} is accepted as a synonym of {@code EXCEPT} (Oracle/GaussDB idiom).</li>
 *   <li>{@code NULLS FIRST/LAST} is always permitted.</li>
 * </ul>
 *
 * <p>Downstream we always emit explicit {@code NULLS FIRST/LAST} on ORDER BY so
 * Spark's null-ordering default does not silently diverge from GaussDB's.
 */
public final class GaussDBSqlConformance extends SqlDelegatingConformance {

  public static final SqlConformance GAUSSDB_SQL = new GaussDBSqlConformance();

  private GaussDBSqlConformance() {
    super(SqlConformanceEnum.PRAGMATIC_2003);
  }

  @Override
  public boolean isMinusAllowed() {
    return true;
  }
}
