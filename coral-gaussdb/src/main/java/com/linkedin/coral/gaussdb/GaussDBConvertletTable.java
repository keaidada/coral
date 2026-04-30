/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb;

import org.apache.calcite.rel.type.RelDataType;
import org.apache.calcite.rex.RexBuilder;
import org.apache.calcite.rex.RexNode;
import org.apache.calcite.sql.SqlCall;
import org.apache.calcite.sql.fun.SqlCastFunction;
import org.apache.calcite.sql.validate.SqlValidator;
import org.apache.calcite.sql2rel.SqlRexContext;

import com.linkedin.coral.hive.hive2rel.CoralConvertletTable;


/**
 * GaussDB-specific convertlet table.
 *
 * <p>Extends {@link CoralConvertletTable} so GaussDB shares the same generic
 * convertlets (function-field references, etc.). The one override tightens
 * {@code CAST} conversion so that the produced {@code RexCall}'s row-type
 * nullability matches what the validator derived for the SqlNode. This is
 * needed because {@link CoralConvertletTable#convertCast} unconditionally
 * passes {@code nullable=true} to {@link org.apache.calcite.sql.SqlDataTypeSpec#deriveType},
 * which collides with the validator's NOT-NULL derivation when the source
 * operand is a NOT-NULL literal (e.g. {@code '123'::int}). Calcite then
 * fails in {@code SqlToRelConverter.checkConvertedType}.
 *
 * <p>We defer to the validator's type whenever it is already known, which is
 * the invariant {@code checkConvertedType} enforces. For any case where the
 * validator has no type (e.g. freshly built AST nodes), we fall back to the
 * old nullable-by-default behavior.
 */
public class GaussDBConvertletTable extends CoralConvertletTable {

  @Override
  @SuppressWarnings("unused")
  public RexNode convertCast(SqlRexContext cx, SqlCastFunction cast, SqlCall call) {
    final SqlValidator validator = cx.getValidator();
    final RelDataType validatedType = validator.getValidatedNodeTypeIfKnown(call);
    final RexNode sourceRex = cx.convertExpression(call.operand(0));
    final RexBuilder rexBuilder = cx.getRexBuilder();
    if (validatedType != null) {
      // Preserve whatever nullability / precision the validator decided — keeps
      // checkConvertedType happy even when the source is NOT NULL.
      return rexBuilder.makeAbstractCast(validatedType, sourceRex);
    }
    // Fallback path — behavior matches CoralConvertletTable.
    return super.convertCast(cx, cast, call);
  }
}
