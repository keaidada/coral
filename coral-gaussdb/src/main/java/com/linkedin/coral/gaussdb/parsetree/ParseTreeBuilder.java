/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb.parsetree;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

import org.antlr.v4.runtime.ParserRuleContext;
import org.antlr.v4.runtime.Token;
import org.antlr.v4.runtime.tree.ParseTree;
import org.antlr.v4.runtime.tree.TerminalNode;
import org.apache.calcite.sql.JoinConditionType;
import org.apache.calcite.sql.JoinType;
import org.apache.calcite.sql.SqlBasicCall;
import org.apache.calcite.sql.SqlIdentifier;
import org.apache.calcite.sql.SqlJoin;
import org.apache.calcite.sql.SqlLiteral;
import org.apache.calcite.sql.SqlNode;
import org.apache.calcite.sql.SqlNodeList;
import org.apache.calcite.sql.SqlNumericLiteral;
import org.apache.calcite.sql.SqlOperator;
import org.apache.calcite.sql.SqlSelect;
import org.apache.calcite.sql.fun.SqlCase;
import org.apache.calcite.sql.fun.SqlStdOperatorTable;
import org.apache.calcite.sql.parser.SqlParserPos;
import org.apache.calcite.sql.type.SqlTypeName;

import com.linkedin.coral.common.calcite.CalciteUtil;
import com.linkedin.coral.gaussdb.parser.GaussDBSqlBaseVisitor;
import com.linkedin.coral.gaussdb.parser.GaussDBSqlParser;


/**
 * ANTLR4 parse tree → Calcite {@link SqlNode}.
 *
 * <p>This is the layer where GaussDB-specific idioms get normalized into
 * dialect-neutral Calcite standard operators so the downstream RelNode IR stays
 * the same as coral-hive / coral-trino produce. Example rewrites:
 *
 * <ul>
 *   <li>{@code a || b} → {@code CONCAT(a, b)}</li>
 *   <li>{@code x::INT} → {@code CAST(x AS INT)}</li>
 *   <li>{@code nvl(a, b)} → {@code COALESCE(a, b)}</li>
 *   <li>{@code sysdate} / {@code now()} → {@code CURRENT_TIMESTAMP}</li>
 * </ul>
 *
 * <p>Per the v1 "hard-fail" policy, any grammar production that this class does
 * not visit yet throws {@link UnhandledASTNodeException} with an input location.
 */
public class ParseTreeBuilder extends GaussDBSqlBaseVisitor<SqlNode> {

  private static final SqlParserPos POS = SqlParserPos.ZERO;

  /**
   * Name given to the synthesized recursive CTE when rewriting
   * {@code START WITH / CONNECT BY}. Any occurrence of {@code PRIOR x}
   * inside the {@code CONNECT BY} predicate is resolved as
   * {@code __coral_connect_by.x}.
   */
  private static final String CONNECT_BY_CTE_NAME = "__coral_connect_by";

  /**
   * Column name used by the DISTINCT ON rewrite for the synthetic
   * {@code ROW_NUMBER()} column. Scoped lowercase (PG folding) so it
   * unambiguously resolves without quoting.
   */
  private static final String DISTINCT_ON_RN_COL = "__coral_distinct_on_rn";

  /** Non-null only while visiting a {@code CONNECT BY} predicate. */
  private String priorAlias = null;

  /* ======================= Top-level ======================= */

  @Override
  public SqlNode visitRoot(GaussDBSqlParser.RootContext ctx) {
    return visit(ctx.statement());
  }

  @Override
  public SqlNode visitStatement(GaussDBSqlParser.StatementContext ctx) {
    if (ctx.selectStatement() != null) {
      return visit(ctx.selectStatement());
    }
    if (ctx.insertStatement() != null) {
      return visit(ctx.insertStatement());
    }
    if (ctx.deleteStatement() != null) {
      return visit(ctx.deleteStatement());
    }
    if (ctx.updateStatement() != null) {
      return visit(ctx.updateStatement());
    }
    if (ctx.mergeStatement() != null) {
      return visit(ctx.mergeStatement());
    }
    throw new UnhandledASTNodeException(ctx, "unknown statement kind");
  }

  @Override
  public SqlNode visitMergeStatement(GaussDBSqlParser.MergeStatementContext ctx) {
    SqlIdentifier target = buildQualifiedIdentifier(ctx.target);
    SqlIdentifier targetAlias = null;
    if (ctx.targetAlias != null) {
      targetAlias = new SqlIdentifier(identifierText(ctx.targetAlias), pos(ctx.targetAlias.start));
    }

    SqlNode source = convertMergeSource(ctx.source);
    if (ctx.sourceAlias != null) {
      SqlIdentifier sa = new SqlIdentifier(identifierText(ctx.sourceAlias), pos(ctx.sourceAlias.start));
      source = SqlStdOperatorTable.AS.createCall(pos(ctx.sourceAlias.start), source, sa);
    }

    SqlNode condition = visit(ctx.cond);

    org.apache.calcite.sql.SqlUpdate updateCall = null;
    org.apache.calcite.sql.SqlInsert insertCall = null;
    for (GaussDBSqlParser.MergeWhenContext w : ctx.mergeWhen()) {
      if (w instanceof GaussDBSqlParser.WhenMatchedUpdateContext) {
        if (updateCall != null) {
          throw new UnhandledASTNodeException(w, "at most one WHEN MATCHED clause is supported in v1");
        }
        GaussDBSqlParser.WhenMatchedUpdateContext u = (GaussDBSqlParser.WhenMatchedUpdateContext) w;
        List<SqlNode> colNodes = new ArrayList<>();
        List<SqlNode> valNodes = new ArrayList<>();
        for (GaussDBSqlParser.AssignmentContext a : u.assignment()) {
          colNodes.add(new SqlIdentifier(identifierText(a.target), pos(a.target.start)));
          valNodes.add(visit(a.value));
        }
        // In a MERGE ... WHEN MATCHED UPDATE, the target-table/alias fields on
        // the nested SqlUpdate are informational only — the outer SqlMerge
        // owns the authoritative target — but Calcite expects non-null
        // placeholders.
        updateCall = new org.apache.calcite.sql.SqlUpdate(pos(u.start), target,
            new SqlNodeList(colNodes, pos(u.start)), new SqlNodeList(valNodes, pos(u.start)),
            /*condition*/ null, /*sourceSelect*/ null, targetAlias);
      } else if (w instanceof GaussDBSqlParser.WhenNotMatchedInsertContext) {
        if (insertCall != null) {
          throw new UnhandledASTNodeException(w, "at most one WHEN NOT MATCHED clause is supported in v1");
        }
        GaussDBSqlParser.WhenNotMatchedInsertContext in = (GaussDBSqlParser.WhenNotMatchedInsertContext) w;
        SqlNodeList columnList = null;
        if (in.insertCols != null && !in.insertCols.isEmpty()) {
          List<SqlNode> cols = new ArrayList<>();
          for (GaussDBSqlParser.IdentifierContext id : in.insertCols) {
            cols.add(new SqlIdentifier(identifierText(id), pos(id.start)));
          }
          columnList = new SqlNodeList(cols, pos(in.start));
        }
        List<SqlNode> valExprs = new ArrayList<>();
        for (GaussDBSqlParser.ExpressionContext e : in.insertVals) {
          valExprs.add(visit(e));
        }
        // Calcite represents INSERT ... VALUES (...) as a SqlSelect over a
        // row constructor wrapped in SqlStdOperatorTable.VALUES. For MERGE's
        // purposes we synthesize that shape.
        SqlNode valuesRow = SqlStdOperatorTable.ROW.createCall(pos(in.start), valExprs.toArray(new SqlNode[0]));
        SqlNode valuesSource =
            SqlStdOperatorTable.VALUES.createCall(pos(in.start), valuesRow);
        insertCall = new org.apache.calcite.sql.SqlInsert(pos(in.start), SqlNodeList.EMPTY, target, valuesSource,
            columnList);
      } else {
        throw new UnhandledASTNodeException(w, "unknown MERGE WHEN branch");
      }
    }

    return new org.apache.calcite.sql.SqlMerge(pos(ctx.start), target, condition, source, updateCall, insertCall,
        /*sourceSelect*/ null, targetAlias);
  }

  private SqlNode convertMergeSource(GaussDBSqlParser.MergeSourceContext ctx) {
    if (ctx.qualifiedName() != null) {
      return buildQualifiedIdentifier(ctx.qualifiedName());
    }
    return visit(ctx.queryExpression());
  }

  @Override
  public SqlNode visitDeleteStatement(GaussDBSqlParser.DeleteStatementContext ctx) {
    SqlIdentifier target = buildQualifiedIdentifier(ctx.qualifiedName());
    SqlIdentifier alias = null;
    if (ctx.alias != null) {
      alias = new SqlIdentifier(identifierText(ctx.alias), pos(ctx.alias.start));
    }
    SqlNode where = ctx.whereClause() != null ? visit(ctx.whereClause().expression()) : null;
    return new org.apache.calcite.sql.SqlDelete(pos(ctx.start), target, where, /*sourceSelect*/ null, alias);
  }

  @Override
  public SqlNode visitUpdateStatement(GaussDBSqlParser.UpdateStatementContext ctx) {
    SqlIdentifier target = buildQualifiedIdentifier(ctx.qualifiedName());
    SqlIdentifier alias = null;
    if (ctx.alias != null) {
      alias = new SqlIdentifier(identifierText(ctx.alias), pos(ctx.alias.start));
    }
    List<SqlNode> colNodes = new ArrayList<>();
    List<SqlNode> valNodes = new ArrayList<>();
    for (GaussDBSqlParser.AssignmentContext a : ctx.assignment()) {
      colNodes.add(new SqlIdentifier(identifierText(a.target), pos(a.target.start)));
      valNodes.add(visit(a.value));
    }
    SqlNodeList targetCols = new SqlNodeList(colNodes, pos(ctx.start));
    SqlNodeList srcExprs = new SqlNodeList(valNodes, pos(ctx.start));
    SqlNode where = ctx.whereClause() != null ? visit(ctx.whereClause().expression()) : null;
    return new org.apache.calcite.sql.SqlUpdate(pos(ctx.start), target, targetCols, srcExprs, where,
        /*sourceSelect*/ null, alias);
  }

  @Override
  public SqlNode visitInsertStatement(GaussDBSqlParser.InsertStatementContext ctx) {
    SqlIdentifier target = buildQualifiedIdentifier(ctx.qualifiedName());
    SqlNodeList columnList = null;
    if (ctx.columns != null && !ctx.columns.isEmpty()) {
      List<SqlNode> cols = new ArrayList<>();
      for (GaussDBSqlParser.IdentifierContext id : ctx.columns) {
        cols.add(new SqlIdentifier(identifierText(id), pos(id.start)));
      }
      columnList = new SqlNodeList(cols, pos(ctx.start));
    }
    SqlNode source = visit(ctx.selectStatement());
    // Empty keyword list = plain INSERT (no OVERWRITE / no APPEND). If GaussDB
    // later needs those keywords, extend here.
    return new org.apache.calcite.sql.SqlInsert(pos(ctx.start), SqlNodeList.EMPTY, target, source, columnList);
  }

  @Override
  public SqlNode visitSelectStatement(GaussDBSqlParser.SelectStatementContext ctx) {
    // DISTINCT ON is a SELECT-level modifier but the rewrite needs access to
    // the enclosing ORDER BY, so we handle it here before delegating.
    GaussDBSqlParser.Select_Context topSelect = findTopLevelSelect(ctx.queryExpression());
    if (topSelect != null && topSelect.distinctOnClause() != null) {
      return rewriteDistinctOn(ctx, topSelect);
    }

    SqlNode body = visit(ctx.queryExpression());

    // Wrap in SqlWith if there's a CTE clause. Per Calcite conventions, ORDER
    // BY / LIMIT / OFFSET go on the outermost SqlOrderBy, which sits OUTSIDE
    // the SqlWith.
    if (ctx.withClause() != null) {
      List<SqlNode> items = new ArrayList<>();
      for (GaussDBSqlParser.NamedQueryContext nq : ctx.withClause().namedQuery()) {
        items.add(visit(nq));
      }
      SqlNodeList withList = new SqlNodeList(items, pos(ctx.withClause().start));
      body = new org.apache.calcite.sql.SqlWith(pos(ctx.withClause().start), withList, body);
    }

    SqlNodeList orderList = SqlNodeList.EMPTY;
    if (ctx.ORDER() != null) {
      List<SqlNode> sortItems = new ArrayList<>();
      for (GaussDBSqlParser.SortItemContext item : ctx.sortItem()) {
        sortItems.add(visit(item));
      }
      orderList = new SqlNodeList(sortItems, pos(ctx.ORDER().getSymbol()));
    }

    SqlNode offset = ctx.offset != null ? visit(ctx.offset) : null;
    SqlNode fetch = ctx.limit != null ? visit(ctx.limit) : null;

    if (orderList == SqlNodeList.EMPTY && offset == null && fetch == null) {
      return body;
    }
    // Wrap with ORDER BY / OFFSET / FETCH.
    return new org.apache.calcite.sql.SqlOrderBy(pos(ctx.start), body, orderList, offset, fetch);
  }

  /**
   * Walks down a {@code queryExpression} to find a bare {@code select_} when
   * the statement is a single-SELECT (no set-op wrapping, no parens). Used to
   * detect SELECT-level modifiers like DISTINCT ON at the outer-statement
   * layer. Returns {@code null} when the top-level query is a set-op or
   * otherwise compound — DISTINCT ON is a PG SELECT-only feature, so these
   * cases fall through to the normal path.
   */
  private GaussDBSqlParser.Select_Context findTopLevelSelect(
      GaussDBSqlParser.QueryExpressionContext qe) {
    if (qe instanceof GaussDBSqlParser.QueryTermDefaultContext) {
      GaussDBSqlParser.QueryTermContext qt = ((GaussDBSqlParser.QueryTermDefaultContext) qe).queryTerm();
      if (qt instanceof GaussDBSqlParser.QueryPrimaryDefaultContext) {
        GaussDBSqlParser.QueryPrimaryContext qp =
            ((GaussDBSqlParser.QueryPrimaryDefaultContext) qt).queryPrimary();
        if (qp instanceof GaussDBSqlParser.QueryPrimarySelectContext) {
          return ((GaussDBSqlParser.QueryPrimarySelectContext) qp).select_();
        }
      }
    }
    return null;
  }

  /**
   * Rewrites {@code SELECT DISTINCT ON (e1, ...) proj FROM src [WHERE w]
   * [ORDER BY o1, ...]} into
   *
   * <pre>
   *   SELECT proj FROM (
   *     SELECT proj, ROW_NUMBER() OVER (
   *       PARTITION BY e1, ...
   *       ORDER BY o1, ...
   *     ) AS __coral_distinct_on_rn
   *     FROM src [WHERE w]
   *   ) __coral_distinct_on_sub
   *   WHERE __coral_distinct_on_rn = 1
   * </pre>
   *
   * <p>DISTINCT ON semantics: among rows sharing {@code (e1, ...)}, keep the
   * first per the provided ORDER BY. When no ORDER BY is given, the "first"
   * is implementation-defined — we still rewrite, producing deterministic but
   * arbitrary ordering (PG behaves the same).
   */
  private SqlNode rewriteDistinctOn(GaussDBSqlParser.SelectStatementContext stmtCtx,
      GaussDBSqlParser.Select_Context selCtx) {
    // PARTITION BY expressions — the DISTINCT ON keys.
    List<SqlNode> partitionExprs = new ArrayList<>();
    for (GaussDBSqlParser.ExpressionContext e : selCtx.distinctOnClause().expression()) {
      partitionExprs.add(visit(e));
    }
    SqlNodeList partitionList = new SqlNodeList(partitionExprs, POS);

    // ORDER BY expressions for the ranking — taken from the outer statement.
    List<SqlNode> orderExprs = new ArrayList<>();
    if (stmtCtx.ORDER() != null) {
      for (GaussDBSqlParser.SortItemContext si : stmtCtx.sortItem()) {
        orderExprs.add(visit(si));
      }
    }
    SqlNodeList orderList = new SqlNodeList(orderExprs, POS);

    // Build ROW_NUMBER() OVER (...) AS __rn.
    SqlNode rowNumber = SqlStdOperatorTable.ROW_NUMBER.createCall(POS);
    SqlLiteral isRows = SqlLiteral.createBoolean(false, POS);
    SqlNode window = org.apache.calcite.sql.SqlWindow.create(null, null, partitionList, orderList,
        isRows, null, null, null, POS);
    SqlNode rnExpr = SqlStdOperatorTable.OVER.createCall(POS, rowNumber, window);
    SqlIdentifier rnAlias = new SqlIdentifier(DISTINCT_ON_RN_COL, POS);
    SqlNode rnAliased = SqlStdOperatorTable.AS.createCall(POS, rnExpr, rnAlias);

    // Inner SELECT: visit the original proj list WITHOUT the DISTINCT ON
    // modifier, then append the RN column. We temporarily build a select
    // without invoking visitSelect_ (which would short-circuit on DISTINCT ON).
    SqlNode innerSelect = buildSelectForDistinctOnInner(selCtx, rnAliased);

    // Outer SELECT: pull the user's original projection columns out of the
    // subquery, filter to __rn = 1. We rebuild the projections rather than
    // "SELECT *" to preserve user-intended aliases / expressions.
    SqlNode innerAlias = new SqlIdentifier("__coral_distinct_on_sub", POS);
    SqlNode wrappedFrom = SqlStdOperatorTable.AS.createCall(POS, innerSelect, innerAlias);

    List<SqlNode> outerProj = new ArrayList<>();
    for (GaussDBSqlParser.SelectItemContext item : selCtx.selectItem()) {
      outerProj.add(visit(item));
    }
    SqlNodeList outerProjList = new SqlNodeList(outerProj, POS);

    SqlNode rnCol = new SqlIdentifier(DISTINCT_ON_RN_COL, POS);
    SqlNode one = SqlNumericLiteral.createExactNumeric("1", POS);
    SqlNode where = SqlStdOperatorTable.EQUALS.createCall(POS, rnCol, one);

    SqlNode outerSelect = new SqlSelect(pos(stmtCtx.start), SqlNodeList.EMPTY, outerProjList, wrappedFrom,
        where, /*groupBy*/ null, /*having*/ null, /*windowDecls*/ null, /*orderBy*/ null,
        /*offset*/ null, /*fetch*/ null);

    // Preserve outer LIMIT/OFFSET (but ORDER BY was already absorbed into the
    // inner window — re-applying it outside could change result order, so we
    // intentionally drop it here; PG's DISTINCT ON requires this anyway).
    SqlNode offset = stmtCtx.offset != null ? visit(stmtCtx.offset) : null;
    SqlNode fetch = stmtCtx.limit != null ? visit(stmtCtx.limit) : null;
    if (offset == null && fetch == null) {
      return outerSelect;
    }
    return new org.apache.calcite.sql.SqlOrderBy(pos(stmtCtx.start), outerSelect, SqlNodeList.EMPTY, offset,
        fetch);
  }

  /**
   * Builds the inner SELECT of a DISTINCT ON rewrite: the original FROM /
   * WHERE plus a projection of (original proj, ROW_NUMBER() AS __rn).
   */
  private SqlNode buildSelectForDistinctOnInner(GaussDBSqlParser.Select_Context ctx, SqlNode rnAliased) {
    List<SqlNode> projections = new ArrayList<>();
    for (GaussDBSqlParser.SelectItemContext item : ctx.selectItem()) {
      projections.add(visit(item));
    }
    projections.add(rnAliased);
    SqlNodeList projList = new SqlNodeList(projections, pos(ctx.SELECT().getSymbol()));

    SqlNode from = ctx.fromClause() != null ? visit(ctx.fromClause()) : null;
    SqlNode where = ctx.whereClause() != null ? visit(ctx.whereClause().expression()) : null;

    return new SqlSelect(pos(ctx.SELECT().getSymbol()), SqlNodeList.EMPTY, projList, from, where,
        /*groupBy*/ null, /*having*/ null, /*windowDecls*/ null, /*orderBy*/ null,
        /*offset*/ null, /*fetch*/ null);
  }

  @Override
  public SqlNode visitNamedQuery(GaussDBSqlParser.NamedQueryContext ctx) {
    SqlIdentifier name = new SqlIdentifier(identifierText(ctx.name), pos(ctx.name.start));
    SqlNodeList cols = null;
    if (ctx.columnAliases != null && !ctx.columnAliases.isEmpty()) {
      List<SqlNode> colIds = new ArrayList<>();
      for (GaussDBSqlParser.IdentifierContext id : ctx.columnAliases) {
        colIds.add(new SqlIdentifier(identifierText(id), pos(id.start)));
      }
      cols = new SqlNodeList(colIds, pos(ctx.start));
    }
    SqlNode def = visit(ctx.queryExpression());
    return new org.apache.calcite.sql.SqlWithItem(pos(ctx.start), name, cols, def);
  }

  /* ======================= Set operations (UNION/INTERSECT/EXCEPT) ======================= */

  @Override
  public SqlNode visitQueryTermDefault(GaussDBSqlParser.QueryTermDefaultContext ctx) {
    return visit(ctx.queryTerm());
  }

  @Override
  public SqlNode visitSetOpUnionExcept(GaussDBSqlParser.SetOpUnionExceptContext ctx) {
    SqlNode left = visit(ctx.left);
    SqlNode right = visit(ctx.right);
    boolean all = ctx.setQuantifier() != null && ctx.setQuantifier().ALL() != null;
    org.apache.calcite.sql.SqlOperator op;
    switch (ctx.op.getType()) {
      case GaussDBSqlParser.UNION:
        op = all ? SqlStdOperatorTable.UNION_ALL : SqlStdOperatorTable.UNION;
        break;
      case GaussDBSqlParser.EXCEPT:
      case GaussDBSqlParser.MINUS_KW:
        // GaussDB MINUS is Oracle-style synonym for EXCEPT.
        op = all ? SqlStdOperatorTable.EXCEPT_ALL : SqlStdOperatorTable.EXCEPT;
        break;
      default:
        throw new UnhandledASTNodeException(ctx, "unsupported set-op token: " + ctx.op.getType());
    }
    return op.createCall(pos(ctx.op), left, right);
  }

  @Override
  public SqlNode visitQueryPrimaryDefault(GaussDBSqlParser.QueryPrimaryDefaultContext ctx) {
    return visit(ctx.queryPrimary());
  }

  @Override
  public SqlNode visitSetOpIntersect(GaussDBSqlParser.SetOpIntersectContext ctx) {
    SqlNode left = visit(ctx.left);
    SqlNode right = visit(ctx.right);
    boolean all = ctx.setQuantifier() != null && ctx.setQuantifier().ALL() != null;
    org.apache.calcite.sql.SqlOperator op =
        all ? SqlStdOperatorTable.INTERSECT_ALL : SqlStdOperatorTable.INTERSECT;
    return op.createCall(pos(ctx.INTERSECT().getSymbol()), left, right);
  }

  @Override
  public SqlNode visitQueryPrimarySelect(GaussDBSqlParser.QueryPrimarySelectContext ctx) {
    return visit(ctx.select_());
  }

  @Override
  public SqlNode visitQueryPrimaryParens(GaussDBSqlParser.QueryPrimaryParensContext ctx) {
    return visit(ctx.queryExpression());
  }

  @Override
  public SqlNode visitQueryPrimaryValues(GaussDBSqlParser.QueryPrimaryValuesContext ctx) {
    return visit(ctx.valuesClause());
  }

  @Override
  public SqlNode visitValuesClause(GaussDBSqlParser.ValuesClauseContext ctx) {
    List<SqlNode> rows = new ArrayList<>();
    for (GaussDBSqlParser.ValuesRowContext rc : ctx.valuesRow()) {
      List<SqlNode> values = new ArrayList<>();
      for (GaussDBSqlParser.ExpressionContext e : rc.expression()) {
        values.add(visit(e));
      }
      rows.add(SqlStdOperatorTable.ROW.createCall(pos(rc.start), values.toArray(new SqlNode[0])));
    }
    return SqlStdOperatorTable.VALUES.createCall(pos(ctx.start), rows.toArray(new SqlNode[0]));
  }

  @Override
  public SqlNode visitSelect_(GaussDBSqlParser.Select_Context ctx) {
    if (ctx.distinctOnClause() != null) {
      // The outer visitSelectStatement handles DISTINCT ON end-to-end
      // (because the rewrite needs access to the outer ORDER BY). Reaching
      // this branch means DISTINCT ON appeared inside a set-op or parenthesised
      // subquery where it cannot see the statement-level ORDER BY — the
      // semantics are ambiguous, so we fail rather than emit wrong SQL.
      throw new UnsupportedOperationException(
          "DISTINCT ON inside a set-op / subquery is not supported; move it to the top-level SELECT");
    }
    if (ctx.connectByClause() != null || ctx.startWithClause() != null) {
      return rewriteConnectByAsRecursiveCte(ctx);
    }
    return buildPlainSelect(ctx, /*fromOverride*/ null);
  }

  private SqlNode buildPlainSelect(GaussDBSqlParser.Select_Context ctx, SqlNode fromOverride) {
    SqlNodeList keywordList = SqlNodeList.EMPTY;
    if (ctx.setQuantifier() != null && ctx.setQuantifier().DISTINCT() != null) {
      keywordList = new SqlNodeList(
          Collections.singletonList(org.apache.calcite.sql.SqlSelectKeyword.DISTINCT.symbol(POS)), POS);
    }

    List<SqlNode> projections = new ArrayList<>();
    for (GaussDBSqlParser.SelectItemContext item : ctx.selectItem()) {
      projections.add(visit(item));
    }
    SqlNodeList projList = new SqlNodeList(projections, pos(ctx.SELECT().getSymbol()));

    SqlNode from = fromOverride != null ? fromOverride
        : (ctx.fromClause() != null ? visit(ctx.fromClause()) : null);
    SqlNode where = ctx.whereClause() != null ? visit(ctx.whereClause().expression()) : null;

    SqlNodeList groupBy = null;
    if (ctx.groupByClause() != null) {
      List<SqlNode> items = new ArrayList<>();
      for (GaussDBSqlParser.ExpressionContext e : ctx.groupByClause().expression()) {
        items.add(visit(e));
      }
      groupBy = new SqlNodeList(items, pos(ctx.groupByClause().start));
    }
    SqlNode having = ctx.havingClause() != null ? visit(ctx.havingClause().expression()) : null;

    return new SqlSelect(pos(ctx.SELECT().getSymbol()), keywordList, projList, from, where, groupBy,
        having, /*windowDecls*/ null, /*orderBy*/ null, /*offset*/ null, /*fetch*/ null);
  }

  /**
   * Rewrites {@code SELECT ... FROM t [WHERE w] START WITH a CONNECT BY b}
   * into a recursive CTE of the form:
   *
   * <pre>
   *   WITH RECURSIVE __coral_connect_by AS (
   *     SELECT t.* FROM t WHERE a            -- anchor row set
   *     UNION ALL
   *     SELECT t.* FROM t, __coral_connect_by WHERE (b with PRIOR x → __coral_connect_by.x)
   *   )
   *   SELECT ... FROM __coral_connect_by [WHERE w]
   * </pre>
   *
   * <p>v1 limits: single source table in the FROM (no joins); no NOCYCLE,
   * no ORDER SIBLINGS BY, no {@code LEVEL} / {@code CONNECT_BY_ROOT} /
   * {@code SYS_CONNECT_BY_PATH}. Any of those unsupported features cause
   * {@link UnsupportedOperationException} with a pointer to the relevant
   * plan milestone.
   */
  private SqlNode rewriteConnectByAsRecursiveCte(GaussDBSqlParser.Select_Context ctx) {
    if (ctx.fromClause() == null) {
      throw new UnsupportedOperationException("CONNECT BY requires a FROM clause");
    }
    if (ctx.fromClause().relation().size() != 1
        || ctx.fromClause().relation(0).joinClause().size() != 0) {
      throw new UnsupportedOperationException(
          "v1 CONNECT BY only supports a single-table FROM (no joins or multi-source)");
    }
    if (ctx.connectByClause() != null && ctx.connectByClause().NOCYCLE() != null) {
      throw new UnsupportedOperationException("NOCYCLE is not supported in v1");
    }
    if (ctx.startWithClause() == null || ctx.connectByClause() == null) {
      throw new UnsupportedOperationException("CONNECT BY must be paired with START WITH in v1");
    }

    GaussDBSqlParser.TableRefContext tbl = ctx.fromClause().relation(0).left;
    SqlIdentifier tableId = buildQualifiedIdentifier(tbl.qualifiedName());
    String tableRefName = tbl.alias != null ? identifierText(tbl.alias)
        : tableId.names.get(tableId.names.size() - 1);

    // Anchor: SELECT <tableRef>.* FROM <tableRef> WHERE <startWith>
    SqlNode anchor = buildSingleTableSelect(tableRefName, tableId, tbl.alias,
        visit(ctx.startWithClause().expression()), pos(ctx.start));

    // Recursive step: translate CONNECT BY predicate with PRIOR rewrite, then
    // FROM <tableRef>, <cte>.
    priorAlias = CONNECT_BY_CTE_NAME;
    SqlNode connectByPred;
    try {
      connectByPred = visit(ctx.connectByClause().expression());
    } finally {
      priorAlias = null;
    }

    SqlNode cteRef = new SqlIdentifier(CONNECT_BY_CTE_NAME, POS);
    SqlNode recursiveFrom = new SqlJoin(POS,
        tbl.alias != null ? SqlStdOperatorTable.AS.createCall(POS, tableId,
            new SqlIdentifier(tableRefName, POS)) : tableId,
        SqlLiteral.createBoolean(false, POS), JoinType.COMMA.symbol(POS), cteRef,
        JoinConditionType.NONE.symbol(POS), null);

    SqlNode recursiveStep = buildStarSelect(tableRefName, recursiveFrom, connectByPred, pos(ctx.start));

    // Combine with UNION ALL.
    SqlNode recursiveDef = SqlStdOperatorTable.UNION_ALL.createCall(POS, anchor, recursiveStep);

    // Build the SqlWith around it. Note: Calcite's SqlWithItem doesn't carry
    // the "RECURSIVE" keyword explicitly; the ability to self-reference comes
    // from the validator detecting the CTE name inside its own definition.
    SqlIdentifier cteName = new SqlIdentifier(CONNECT_BY_CTE_NAME, POS);
    SqlNode withItem = new org.apache.calcite.sql.SqlWithItem(POS, cteName, null, recursiveDef);
    SqlNodeList withList = new SqlNodeList(Collections.singletonList(withItem), POS);

    // Outer SELECT drives the projection. FROM is the CTE. Existing WHERE
    // (if any) from the original SELECT still applies outside the recursion.
    SqlNode outerFrom = new SqlIdentifier(CONNECT_BY_CTE_NAME, POS);
    SqlNode outerSelect = buildPlainSelect(ctx, outerFrom);
    return new org.apache.calcite.sql.SqlWith(pos(ctx.start), withList, outerSelect);
  }

  /** Builds {@code SELECT <alias>.* FROM <table> [AS <alias>] [WHERE ...]}. */
  private SqlNode buildSingleTableSelect(String alias, SqlIdentifier tableId,
      GaussDBSqlParser.IdentifierContext aliasCtx, SqlNode where, SqlParserPos pos) {
    SqlNode from = aliasCtx != null
        ? SqlStdOperatorTable.AS.createCall(pos, tableId, new SqlIdentifier(alias, pos))
        : tableId;
    return buildStarSelect(alias, from, where, pos);
  }

  /** Builds {@code SELECT <alias>.* FROM <from> [WHERE ...]}. */
  private SqlNode buildStarSelect(String alias, SqlNode from, SqlNode where, SqlParserPos pos) {
    SqlIdentifier aliasStar =
        SqlIdentifier.star(java.util.Arrays.asList(alias, ""), pos, Collections.nCopies(2, POS));
    SqlNodeList projList = new SqlNodeList(Collections.singletonList(aliasStar), pos);
    return new SqlSelect(pos, SqlNodeList.EMPTY, projList, from, where, /*groupBy*/ null,
        /*having*/ null, /*windowDecls*/ null, /*orderBy*/ null, /*offset*/ null, /*fetch*/ null);
  }

  @Override
  public SqlNode visitPriorColumnReference(GaussDBSqlParser.PriorColumnReferenceContext ctx) {
    if (priorAlias == null) {
      throw new UnsupportedOperationException("PRIOR may only appear inside a CONNECT BY clause");
    }
    // PRIOR x → <cte>.x. For qualified PRIOR a.b, we use <cte>.b (PG-style
    // — the PRIOR scope is the recursive CTE, so any table alias is dropped).
    List<String> names = new ArrayList<>();
    names.add(priorAlias);
    List<GaussDBSqlParser.IdentifierContext> ids = ctx.qualifiedName().identifier();
    names.add(identifierText(ids.get(ids.size() - 1)));
    return new SqlIdentifier(names, pos(ctx.start));
  }

  @Override
  public SqlNode visitFromClause(GaussDBSqlParser.FromClauseContext ctx) {
    List<GaussDBSqlParser.RelationContext> rels = ctx.relation();
    SqlNode left = visit(rels.get(0));
    for (int i = 1; i < rels.size(); i++) {
      SqlNode right = visit(rels.get(i));
      left = new SqlJoin(POS, left, SqlLiteral.createBoolean(false, POS), JoinType.COMMA.symbol(POS), right,
          JoinConditionType.NONE.symbol(POS), null);
    }
    return left;
  }

  @Override
  public SqlNode visitRelation(GaussDBSqlParser.RelationContext ctx) {
    SqlNode left = visit(ctx.left);
    for (GaussDBSqlParser.JoinClauseContext jc : ctx.joinClause()) {
      left = applyJoin(left, jc);
    }
    return left;
  }

  private SqlNode applyJoin(SqlNode left, GaussDBSqlParser.JoinClauseContext jc) {
    if (jc instanceof GaussDBSqlParser.CrossJoinContext) {
      GaussDBSqlParser.CrossJoinContext cj = (GaussDBSqlParser.CrossJoinContext) jc;
      SqlNode right = visit(cj.tableRef());
      return new SqlJoin(pos(cj.start), left, SqlLiteral.createBoolean(false, POS), JoinType.CROSS.symbol(POS), right,
          JoinConditionType.NONE.symbol(POS), null);
    }
    if (jc instanceof GaussDBSqlParser.QualifiedJoinContext) {
      GaussDBSqlParser.QualifiedJoinContext qj = (GaussDBSqlParser.QualifiedJoinContext) jc;
      JoinType joinType = JoinType.INNER;
      switch (qj.joinType.getType()) {
        case GaussDBSqlParser.LEFT:
          joinType = JoinType.LEFT;
          break;
        case GaussDBSqlParser.RIGHT:
          joinType = JoinType.RIGHT;
          break;
        case GaussDBSqlParser.FULL:
          joinType = JoinType.FULL;
          break;
        case GaussDBSqlParser.INNER:
        default:
          joinType = JoinType.INNER;
      }
      SqlNode right = visit(qj.tableRef());
      return buildJoinWithCriteria(left, right, joinType, qj.joinCriteria(), pos(qj.start));
    }
    if (jc instanceof GaussDBSqlParser.DefaultInnerJoinContext) {
      GaussDBSqlParser.DefaultInnerJoinContext dj = (GaussDBSqlParser.DefaultInnerJoinContext) jc;
      SqlNode right = visit(dj.tableRef());
      return buildJoinWithCriteria(left, right, JoinType.INNER, dj.joinCriteria(), pos(dj.start));
    }
    throw new UnhandledASTNodeException(jc, "unknown join clause");
  }

  private SqlNode buildJoinWithCriteria(SqlNode left, SqlNode right, JoinType joinType,
      GaussDBSqlParser.JoinCriteriaContext criteria, SqlParserPos joinPos) {
    if (criteria instanceof GaussDBSqlParser.JoinOnContext) {
      SqlNode on = visit(((GaussDBSqlParser.JoinOnContext) criteria).expression());
      return new SqlJoin(joinPos, left, SqlLiteral.createBoolean(false, POS), joinType.symbol(POS), right,
          JoinConditionType.ON.symbol(POS), on);
    }
    if (criteria instanceof GaussDBSqlParser.JoinUsingContext) {
      GaussDBSqlParser.JoinUsingContext ju = (GaussDBSqlParser.JoinUsingContext) criteria;
      List<SqlNode> colIds = new ArrayList<>();
      for (GaussDBSqlParser.IdentifierContext id : ju.identifier()) {
        colIds.add(new SqlIdentifier(identifierText(id), pos(id.start)));
      }
      SqlNodeList usingList = new SqlNodeList(colIds, pos(criteria.start));
      return new SqlJoin(joinPos, left, SqlLiteral.createBoolean(false, POS), joinType.symbol(POS), right,
          JoinConditionType.USING.symbol(POS), usingList);
    }
    throw new UnhandledASTNodeException(criteria, "unknown join criteria");
  }

  @Override
  public SqlNode visitTableRef(GaussDBSqlParser.TableRefContext ctx) {
    SqlNode table = buildQualifiedIdentifier(ctx.qualifiedName());
    if (ctx.alias != null) {
      SqlIdentifier aliasId = new SqlIdentifier(identifierText(ctx.alias), pos(ctx.alias.start));
      return SqlStdOperatorTable.AS.createCall(pos(ctx.start), table, aliasId);
    }
    return table;
  }

  /* ======================= Select items ======================= */

  @Override
  public SqlNode visitSelectAll(GaussDBSqlParser.SelectAllContext ctx) {
    return SqlIdentifier.star(pos(ctx.STAR().getSymbol()));
  }

  @Override
  public SqlNode visitSelectQualifiedAll(GaussDBSqlParser.SelectQualifiedAllContext ctx) {
    List<String> names = new ArrayList<>();
    for (GaussDBSqlParser.IdentifierContext id : ctx.qualifiedName().identifier()) {
      names.add(identifierText(id));
    }
    names.add("");
    return SqlIdentifier.star(names, pos(ctx.start), Collections.nCopies(names.size(), POS));
  }

  @Override
  public SqlNode visitSelectExpression(GaussDBSqlParser.SelectExpressionContext ctx) {
    SqlNode expr = visit(ctx.expression());
    if (ctx.alias != null) {
      SqlIdentifier aliasId = new SqlIdentifier(identifierText(ctx.alias), pos(ctx.alias.start));
      return SqlStdOperatorTable.AS.createCall(pos(ctx.start), expr, aliasId);
    }
    return expr;
  }

  @Override
  public SqlNode visitSortItem(GaussDBSqlParser.SortItemContext ctx) {
    SqlNode expr = visit(ctx.expression());
    if (ctx.ordering != null && ctx.ordering.getType() == GaussDBSqlParser.DESC) {
      expr = SqlStdOperatorTable.DESC.createCall(POS, expr);
    }
    if (ctx.nullOrder != null) {
      if (ctx.nullOrder.getType() == GaussDBSqlParser.FIRST) {
        expr = SqlStdOperatorTable.NULLS_FIRST.createCall(POS, expr);
      } else {
        expr = SqlStdOperatorTable.NULLS_LAST.createCall(POS, expr);
      }
    }
    return expr;
  }

  /* ======================= Expressions ======================= */

  @Override
  public SqlNode visitOrExpr(GaussDBSqlParser.OrExprContext ctx) {
    return SqlStdOperatorTable.OR.createCall(pos(ctx.start), visit(ctx.expression(0)), visit(ctx.expression(1)));
  }

  @Override
  public SqlNode visitAndExpr(GaussDBSqlParser.AndExprContext ctx) {
    return SqlStdOperatorTable.AND.createCall(pos(ctx.start), visit(ctx.expression(0)), visit(ctx.expression(1)));
  }

  @Override
  public SqlNode visitNotExpr(GaussDBSqlParser.NotExprContext ctx) {
    return SqlStdOperatorTable.NOT.createCall(pos(ctx.start), visit(ctx.expression()));
  }

  @Override
  public SqlNode visitPredicateDefault(GaussDBSqlParser.PredicateDefaultContext ctx) {
    return visit(ctx.predicate());
  }

  @Override
  public SqlNode visitComparisonPredicate(GaussDBSqlParser.ComparisonPredicateContext ctx) {
    SqlOperator op = comparisonOp(ctx.comparisonOp().start.getType());
    return op.createCall(pos(ctx.start), visit(ctx.valueExpression(0)), visit(ctx.valueExpression(1)));
  }

  @Override
  public SqlNode visitIsNullPredicate(GaussDBSqlParser.IsNullPredicateContext ctx) {
    SqlOperator op = ctx.NOT() != null ? SqlStdOperatorTable.IS_NOT_NULL : SqlStdOperatorTable.IS_NULL;
    return op.createCall(pos(ctx.start), visit(ctx.valueExpression()));
  }

  @Override
  public SqlNode visitBetweenPredicate(GaussDBSqlParser.BetweenPredicateContext ctx) {
    SqlNode value = visit(ctx.valueExpression(0));
    SqlNode lower = visit(ctx.valueExpression(1));
    SqlNode upper = visit(ctx.valueExpression(2));
    SqlOperator op = ctx.NOT() != null ? SqlStdOperatorTable.NOT_BETWEEN : SqlStdOperatorTable.BETWEEN;
    return op.createCall(pos(ctx.start), value, lower, upper);
  }

  @Override
  public SqlNode visitInListPredicate(GaussDBSqlParser.InListPredicateContext ctx) {
    SqlNode value = visit(ctx.valueExpression());
    List<SqlNode> items = new ArrayList<>();
    for (GaussDBSqlParser.LiteralContext lit : ctx.literalList().literal()) {
      items.add(visit(lit));
    }
    SqlNodeList list = new SqlNodeList(items, pos(ctx.start));
    SqlOperator op = ctx.NOT() != null ? SqlStdOperatorTable.NOT_IN : SqlStdOperatorTable.IN;
    return op.createCall(pos(ctx.start), value, list);
  }

  @Override
  public SqlNode visitInSubqueryPredicate(GaussDBSqlParser.InSubqueryPredicateContext ctx) {
    // x IN (SELECT ...) — Calcite represents this as IN / NOT IN with the
    // right operand being a sub-select SqlNode.
    SqlNode value = visit(ctx.valueExpression());
    SqlNode subquery = visit(ctx.queryExpression());
    SqlOperator op = ctx.NOT() != null ? SqlStdOperatorTable.NOT_IN : SqlStdOperatorTable.IN;
    return op.createCall(pos(ctx.start), value, subquery);
  }

  @Override
  public SqlNode visitExistsPredicate(GaussDBSqlParser.ExistsPredicateContext ctx) {
    SqlNode subquery = visit(ctx.queryExpression());
    SqlNode exists = SqlStdOperatorTable.EXISTS.createCall(pos(ctx.EXISTS().getSymbol()), subquery);
    if (ctx.NOT() != null) {
      return SqlStdOperatorTable.NOT.createCall(pos(ctx.start), exists);
    }
    return exists;
  }

  @Override
  public SqlNode visitRegexPredicate(GaussDBSqlParser.RegexPredicateContext ctx) {
    SqlNode left = visit(ctx.valueExpression(0));
    SqlNode right = visit(ctx.valueExpression(1));
    int opType = ctx.regexOp().start.getType();

    boolean negated = opType == GaussDBSqlParser.BANG_TILDE
        || opType == GaussDBSqlParser.BANG_TILDE_STAR;
    boolean caseInsensitive = opType == GaussDBSqlParser.TILDE_STAR
        || opType == GaussDBSqlParser.BANG_TILDE_STAR;

    // Hive/Spark have no case-insensitive RLIKE operator. Fold both sides to
    // lowercase so the comparison is case-insensitive. This matches
    // PostgreSQL's ~* semantics for ASCII inputs (Unicode folding is a known
    // lossy area tracked in GRAMMAR_COVERAGE.md).
    if (caseInsensitive) {
      left = emitUnresolved("lower", pos(ctx.start), new SqlNode[] { left });
      right = emitUnresolved("lower", pos(ctx.start), new SqlNode[] { right });
    }

    SqlNode match = emitUnresolved("rlike", pos(ctx.start), new SqlNode[] { left, right });
    return negated ? SqlStdOperatorTable.NOT.createCall(pos(ctx.start), match) : match;
  }

  @Override
  public SqlNode visitScalarSubquery(GaussDBSqlParser.ScalarSubqueryContext ctx) {
    // A bare (SELECT ...) used in expression position. Calcite accepts the
    // SqlSelect (or SqlWith / SqlOrderBy wrapper) directly as an operand.
    return visit(ctx.queryExpression());
  }

  @Override
  public SqlNode visitValueExpressionDefault(GaussDBSqlParser.ValueExpressionDefaultContext ctx) {
    return visit(ctx.valueExpression());
  }

  @Override
  public SqlNode visitArithmeticMulDiv(GaussDBSqlParser.ArithmeticMulDivContext ctx) {
    SqlOperator op;
    switch (ctx.op.getType()) {
      case GaussDBSqlParser.STAR:
        op = SqlStdOperatorTable.MULTIPLY;
        break;
      case GaussDBSqlParser.SLASH:
        op = SqlStdOperatorTable.DIVIDE;
        break;
      case GaussDBSqlParser.PERCENT:
        op = SqlStdOperatorTable.MOD;
        break;
      default:
        throw new UnhandledASTNodeException(ctx, "unknown mul/div operator");
    }
    return op.createCall(pos(ctx.op), visit(ctx.valueExpression(0)), visit(ctx.valueExpression(1)));
  }

  @Override
  public SqlNode visitArithmeticAddSub(GaussDBSqlParser.ArithmeticAddSubContext ctx) {
    SqlOperator op =
        ctx.op.getType() == GaussDBSqlParser.PLUS ? SqlStdOperatorTable.PLUS : SqlStdOperatorTable.MINUS;
    return op.createCall(pos(ctx.op), visit(ctx.valueExpression(0)), visit(ctx.valueExpression(1)));
  }

  @Override
  public SqlNode visitStringConcat(GaussDBSqlParser.StringConcatContext ctx) {
    // GaussDB `a || b` is string concatenation. Normalize to CONCAT so Spark's
    // concat(...) renders cleanly.
    return SqlStdOperatorTable.CONCAT.createCall(pos(ctx.CONCAT_OP().getSymbol()), visit(ctx.valueExpression(0)),
        visit(ctx.valueExpression(1)));
  }

  @Override
  public SqlNode visitPgCastExpression(GaussDBSqlParser.PgCastExpressionContext ctx) {
    // `x::type` → CAST(x AS type)
    SqlNode value = visit(ctx.valueExpression());
    SqlNode typeSpec = buildTypeSpec(ctx.typeName());
    return SqlStdOperatorTable.CAST.createCall(pos(ctx.DOUBLE_COLON().getSymbol()), value, typeSpec);
  }

  @Override
  public SqlNode visitUnarySign(GaussDBSqlParser.UnarySignContext ctx) {
    SqlOperator op = ctx.op.getType() == GaussDBSqlParser.PLUS ? SqlStdOperatorTable.UNARY_PLUS
        : SqlStdOperatorTable.UNARY_MINUS;
    return op.createCall(pos(ctx.op), visit(ctx.valueExpression()));
  }

  @Override
  public SqlNode visitPrimaryDefault(GaussDBSqlParser.PrimaryDefaultContext ctx) {
    return visit(ctx.primaryExpression());
  }

  @Override
  public SqlNode visitParenthesizedExpression(GaussDBSqlParser.ParenthesizedExpressionContext ctx) {
    return visit(ctx.expression());
  }

  @Override
  public SqlNode visitColumnReference(GaussDBSqlParser.ColumnReferenceContext ctx) {
    return buildQualifiedIdentifier(ctx.qualifiedName());
  }

  @Override
  public SqlNode visitOracleOuterJoinColumnReference(
      GaussDBSqlParser.OracleOuterJoinColumnReferenceContext ctx) {
    // Parsed so the user gets an actionable error rather than a cryptic
    // syntax failure. ANSI JOIN is the path forward — (+) is ambiguous for
    // multi-table predicates and not supported by Spark.
    throw new UnsupportedOperationException(
        "Oracle legacy outer-join marker '(+)' is not supported — rewrite as ANSI LEFT/RIGHT JOIN "
            + "(found at " + ctx.qualifiedName().getText() + "(+))");
  }

  @Override
  public SqlNode visitCaseExpr(GaussDBSqlParser.CaseExprContext ctx) {
    return visit(ctx.caseExpression());
  }

  @Override
  public SqlNode visitCaseExpression(GaussDBSqlParser.CaseExpressionContext ctx) {
    SqlNode operand = ctx.operand != null ? visit(ctx.operand) : null;
    List<SqlNode> whenList = new ArrayList<>();
    List<SqlNode> thenList = new ArrayList<>();
    for (GaussDBSqlParser.WhenClauseContext w : ctx.whenClause()) {
      whenList.add(visit(w.condition));
      thenList.add(visit(w.result));
    }
    SqlNode elseExpr = ctx.elseExpr != null ? visit(ctx.elseExpr) : SqlLiteral.createNull(POS);
    return new SqlCase(pos(ctx.start), operand, new SqlNodeList(whenList, POS), new SqlNodeList(thenList, POS),
        elseExpr);
  }

  @Override
  public SqlNode visitStandardCastExpr(GaussDBSqlParser.StandardCastExprContext ctx) {
    return visit(ctx.castExpression());
  }

  @Override
  public SqlNode visitCastExpression(GaussDBSqlParser.CastExpressionContext ctx) {
    return SqlStdOperatorTable.CAST.createCall(pos(ctx.start), visit(ctx.expression()), buildTypeSpec(ctx.typeName()));
  }

  @Override
  public SqlNode visitFunctionExpr(GaussDBSqlParser.FunctionExprContext ctx) {
    return visit(ctx.functionCall());
  }

  @Override
  public SqlNode visitFunctionCall(GaussDBSqlParser.FunctionCallContext ctx) {
    SqlNode call = buildFunctionCallWithoutOver(ctx);
    if (ctx.overClause() != null) {
      SqlNode window = buildWindow(ctx.overClause());
      return SqlStdOperatorTable.OVER.createCall(pos(ctx.overClause().OVER().getSymbol()), call, window);
    }
    return call;
  }

  private SqlNode buildWindow(GaussDBSqlParser.OverClauseContext ctx) {
    SqlNodeList partition = SqlNodeList.EMPTY;
    if (ctx.partitionBy != null && !ctx.partitionBy.isEmpty()) {
      List<SqlNode> parts = new ArrayList<>();
      for (GaussDBSqlParser.ExpressionContext e : ctx.partitionBy) {
        parts.add(visit(e));
      }
      partition = new SqlNodeList(parts, pos(ctx.start));
    }
    SqlNodeList orderBy = SqlNodeList.EMPTY;
    if (ctx.ORDER() != null) {
      List<SqlNode> items = new ArrayList<>();
      for (GaussDBSqlParser.SortItemContext si : ctx.sortItem()) {
        items.add(visit(si));
      }
      orderBy = new SqlNodeList(items, pos(ctx.ORDER().getSymbol()));
    }

    // Frame clause — optional. Calcite encodes frame as (isRows, lowerBound,
    // upperBound); when absent, SqlWindow uses a default range-based frame.
    SqlLiteral isRows = SqlLiteral.createBoolean(false, POS);
    SqlNode lower = null;
    SqlNode upper = null;
    if (ctx.frameClause() != null) {
      GaussDBSqlParser.FrameClauseContext fc = ctx.frameClause();
      boolean rows;
      GaussDBSqlParser.FrameBoundContext startCtx;
      GaussDBSqlParser.FrameBoundContext endCtx;
      if (fc instanceof GaussDBSqlParser.FrameBetweenContext) {
        GaussDBSqlParser.FrameBetweenContext fb = (GaussDBSqlParser.FrameBetweenContext) fc;
        rows = fb.frameKind.getType() == GaussDBSqlParser.ROWS;
        startCtx = fb.frameStart;
        endCtx = fb.frameEnd;
      } else {
        GaussDBSqlParser.FrameSingleContext fs = (GaussDBSqlParser.FrameSingleContext) fc;
        rows = fs.frameKind.getType() == GaussDBSqlParser.ROWS;
        startCtx = fs.frameStart;
        endCtx = null;
      }
      isRows = SqlLiteral.createBoolean(rows, POS);
      lower = convertFrameBound(startCtx);
      // When only start bound is given, upper defaults to CURRENT ROW.
      upper = endCtx != null ? convertFrameBound(endCtx)
          : org.apache.calcite.sql.SqlWindow.createCurrentRow(POS);
    }

    // Build a SqlWindow (no reference-name, no ALLOW PARTIAL in Stage 4).
    return org.apache.calcite.sql.SqlWindow.create(/*declName*/ null, /*refName*/ null, partition, orderBy,
        isRows, lower, upper, /*allowPartial*/ null, pos(ctx.start));
  }

  private SqlNode convertFrameBound(GaussDBSqlParser.FrameBoundContext ctx) {
    if (ctx instanceof GaussDBSqlParser.FrameUnboundedPrecedingContext) {
      return org.apache.calcite.sql.SqlWindow.createUnboundedPreceding(pos(ctx.start));
    }
    if (ctx instanceof GaussDBSqlParser.FrameUnboundedFollowingContext) {
      return org.apache.calcite.sql.SqlWindow.createUnboundedFollowing(pos(ctx.start));
    }
    if (ctx instanceof GaussDBSqlParser.FrameCurrentRowContext) {
      return org.apache.calcite.sql.SqlWindow.createCurrentRow(pos(ctx.start));
    }
    if (ctx instanceof GaussDBSqlParser.FrameNumPrecedingContext) {
      GaussDBSqlParser.FrameNumPrecedingContext fp = (GaussDBSqlParser.FrameNumPrecedingContext) ctx;
      return org.apache.calcite.sql.SqlWindow.createPreceding(visit(fp.numberLiteral()), pos(ctx.start));
    }
    if (ctx instanceof GaussDBSqlParser.FrameNumFollowingContext) {
      GaussDBSqlParser.FrameNumFollowingContext ff = (GaussDBSqlParser.FrameNumFollowingContext) ctx;
      return org.apache.calcite.sql.SqlWindow.createFollowing(visit(ff.numberLiteral()), pos(ctx.start));
    }
    throw new UnhandledASTNodeException(ctx, "unknown frame bound");
  }

  private SqlNode buildFunctionCallWithoutOver(GaussDBSqlParser.FunctionCallContext ctx) {
    String rawName = extractFunctionName(ctx.functionName());
    String canonical = canonicalFunctionName(rawName);

    // 0-arg name-shaped specials: SYSDATE/CURRENT_TIMESTAMP emit a keyword-style
    // literal, but support () form too for portability.
    List<SqlNode> args = new ArrayList<>();
    boolean starArg = false;
    if (ctx.functionArg() != null) {
      for (GaussDBSqlParser.FunctionArgContext arg : ctx.functionArg()) {
        if (arg.STAR() != null) {
          // e.g. count(*) — Calcite represents this as a zero-arg COUNT call.
          starArg = true;
        } else {
          args.add(visit(arg.expression()));
        }
      }
    }

    // count(*) → COUNT(<star>) — Calcite expects a 1-operand call where the
    // operand is a special "star identifier" (see CalciteUtil.createStarIdentifier).
    if (starArg && "count".equals(canonical) && args.isEmpty()) {
      return SqlStdOperatorTable.COUNT.createCall(pos(ctx.start), CalciteUtil.createStarIdentifier(POS));
    }
    if (starArg) {
      throw new UnhandledASTNodeException(ctx, "'*' argument is only supported inside count(*)");
    }

    // GaussDB-specific rewrites → Calcite standard operators.
    switch (canonical) {
      // ---- Aggregates: emit the Calcite std op directly so the validator
      // ---- recognizes them as agg functions (not UDFs).
      case "sum":
        return SqlStdOperatorTable.SUM.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "avg":
        return SqlStdOperatorTable.AVG.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "min":
        return SqlStdOperatorTable.MIN.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "max":
        return SqlStdOperatorTable.MAX.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "count":
        return SqlStdOperatorTable.COUNT.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- Window/analytical ranking.
      case "row_number":
        return SqlStdOperatorTable.ROW_NUMBER.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "rank":
        return SqlStdOperatorTable.RANK.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "dense_rank":
        return SqlStdOperatorTable.DENSE_RANK.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- String/null helpers: normalized to std ops so downstream stays
      // ---- dialect-neutral (see also registry doc in plan).
      case "coalesce":
        return SqlStdOperatorTable.COALESCE.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "nvl":
        return SqlStdOperatorTable.COALESCE.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));
      case "nvl2":
        if (args.size() != 3) {
          throw new UnhandledASTNodeException(ctx, "nvl2 expects 3 arguments");
        }
        // nvl2(a, b, c) → CASE WHEN a IS NOT NULL THEN b ELSE c END
        SqlNode notNull = SqlStdOperatorTable.IS_NOT_NULL.createCall(POS, args.get(0));
        return new SqlCase(pos(ctx.start), null,
            new SqlNodeList(Collections.singletonList(notNull), POS),
            new SqlNodeList(Collections.singletonList(args.get(1)), POS), args.get(2));
      case "sysdate":
      case "now":
        return SqlStdOperatorTable.CURRENT_TIMESTAMP.createCall(pos(ctx.start));
      case "substr":
        return SqlStdOperatorTable.SUBSTRING.createCall(pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- decode(sel, k1, v1, k2, v2, ..., [default]) → nested CASE WHEN.
      // Oracle/GaussDB form: decode(x, 1, 'a', 2, 'b', 'c') ≡
      // CASE WHEN x = 1 THEN 'a' WHEN x = 2 THEN 'b' ELSE 'c' END.
      case "decode":
        return buildDecode(ctx, args);

      // ---- mod(a, b) → a % b. Spark supports mod() natively too but using
      // the operator keeps IR dialect-neutral and avoids ambiguity with the
      // aggregate-shaped resolver in Hive's registry.
      case "mod":
        if (args.size() != 2) {
          throw new UnhandledASTNodeException(ctx, "mod expects 2 arguments");
        }
        return SqlStdOperatorTable.MOD.createCall(pos(ctx.start), args.get(0), args.get(1));

      // ---- random() → rand(). Spark recognizes both but rand() is the
      // canonical name in Spark SQL.
      case "random":
        return new SqlBasicCall(new org.apache.calcite.sql.SqlUnresolvedFunction(
            new SqlIdentifier("rand", pos(ctx.start)), null, null, null, null,
            org.apache.calcite.sql.SqlFunctionCategory.SYSTEM), args.toArray(new SqlNode[0]), pos(ctx.start));

      // ---- position(a IN b) / position(a, b) → instr(b, a). GaussDB's
      // grammar uses "position(a IN b)" which our .g4 does not yet special-case;
      // users who write position(a, b) get the arg-swap here. The IN form is
      // a Stage-5 grammar item.
      case "position":
        if (args.size() != 2) {
          throw new UnhandledASTNodeException(ctx, "position expects 2 arguments");
        }
        return emitUnresolved("instr", pos(ctx.start),
            new SqlNode[] { args.get(1), args.get(0) });

      // ---- bool_and / bool_or: GaussDB names for EVERY / SOME in SQL std,
      // and for every/any in Spark SQL. The Calcite std table has SOME/EVERY.
      case "bool_and":
        return emitUnresolved("every", pos(ctx.start), args.toArray(new SqlNode[0]));
      case "bool_or":
        return emitUnresolved("some", pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- array_agg(x) → collect_list(x). Coral's Hive function registry
      // already maps collect_list as an aggregate; we emit that name.
      case "array_agg":
        return emitUnresolved("collect_list", pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- string_agg(x, sep) → concat_ws(sep, collect_list(x)). Two-step
      // rewrite: wrap collect_list(x), then concat_ws(sep, ...).
      case "string_agg":
        if (args.size() != 2) {
          throw new UnhandledASTNodeException(ctx, "string_agg expects 2 arguments");
        }
        SqlNode collected = emitUnresolved("collect_list", pos(ctx.start), new SqlNode[] { args.get(0) });
        return emitUnresolved("concat_ws", pos(ctx.start), new SqlNode[] { args.get(1), collected });

      // ---- trunc(date, unit) → date_trunc(unit, date). Note the arg swap.
      // GaussDB also allows trunc(numeric, digits) — that form passes through
      // unchanged because Spark's trunc() with numeric+integer has the same
      // meaning as GaussDB's.
      case "trunc":
        if (args.size() == 2) {
          // Heuristic: if arg2 is a string literal, treat as date_trunc.
          // Numeric-arg case falls through to the plain passthrough below.
          SqlNode second = args.get(1);
          if (second instanceof SqlLiteral
              && ((SqlLiteral) second).getTypeName() == SqlTypeName.CHAR) {
            return emitUnresolved("date_trunc", pos(ctx.start),
                new SqlNode[] { args.get(1), args.get(0) });
          }
        }
        return emitUnresolved("trunc", pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- regexp_substr(str, pattern [, pos [, occurrence]])
      // → regexp_extract(str, pattern, 0). We drop the pos/occurrence args
      // (Spark's regexp_extract takes a capture-group index instead) and fall
      // back to the 0 group. A richer translation (respecting pos/occurrence)
      // is a Stage-4 backend item.
      case "regexp_substr":
        if (args.size() >= 2) {
          return emitUnresolved("regexp_extract", pos(ctx.start),
              new SqlNode[] { args.get(0), args.get(1),
                  SqlNumericLiteral.createExactNumeric("0", pos(ctx.start)) });
        }
        return emitUnresolved("regexp_substr", pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- generate_series(a, b [, step]) → sequence(a, b [, step]).
      // PostgreSQL's generate_series is a set-returning function; Spark's
      // sequence() produces an array that users typically LATERAL VIEW
      // EXPLODE. In scalar positions both surface as arrays of bigints so
      // the rename is direct. GaussDB also has generate_series(start, stop,
      // interval) for timestamps — the same arg order maps cleanly.
      case "generate_series":
        if (args.size() < 2 || args.size() > 3) {
          throw new UnhandledASTNodeException(ctx, "generate_series expects 2 or 3 arguments");
        }
        return emitUnresolved("sequence", pos(ctx.start), args.toArray(new SqlNode[0]));

      // ---- Date/time format token translation.
      // PostgreSQL/GaussDB use case-sensitive tokens (YYYY, MI, HH24, FF3, ...)
      // that differ from Spark's Java DateTimeFormatter tokens. We rewrite
      // the format string literal in-place. If the 2nd arg is not a string
      // literal (e.g. column-valued), the call passes through unchanged —
      // users must translate the format manually in that case.
      case "to_date":
        return emitUnresolved("to_date", pos(ctx.start), translateDateFormatArgs(args));
      case "to_timestamp":
        return emitUnresolved("to_timestamp", pos(ctx.start), translateDateFormatArgs(args));
      case "to_char":
        // to_char has many overloads; for numeric(x, 'fm999.99') we pass through.
        // Only rewrite when arg0 is (likely) a date/timestamp — heuristic: if
        // the 2nd arg format contains a date-ish token, treat as date_format.
        if (args.size() == 2 && args.get(1) instanceof SqlLiteral
            && ((SqlLiteral) args.get(1)).getTypeName() == SqlTypeName.CHAR) {
          String fmt = ((SqlLiteral) args.get(1)).getValueAs(String.class);
          if (containsDateFormatToken(fmt)) {
            SqlNode[] newArgs = new SqlNode[] { args.get(0),
                SqlLiteral.createCharString(translatePgDateFormat(fmt), pos(ctx.start)) };
            return emitUnresolved("date_format", pos(ctx.start), newArgs);
          }
        }
        return emitUnresolved("to_char", pos(ctx.start), args.toArray(new SqlNode[0]));

      default:
        // Fallback: emit as a generic SqlIdentifier-backed SqlBasicCall. The validator
        // will resolve it against the Hive/Dali operator table (which exposes the
        // standard aggregates/string/math library) via ChainedSqlOperatorTable. We
        // deliberately pass SYSTEM as the category so Calcite's validator considers
        // ALL SqlFunctionCategories when resolving — marking as USER_DEFINED_FUNCTION
        // would shortcut the resolver into the wrong branch for builtins like SUM/AVG.
        SqlIdentifier fnId = new SqlIdentifier(canonical, pos(ctx.start));
        return new SqlBasicCall(new org.apache.calcite.sql.SqlUnresolvedFunction(fnId, null, null, null, null,
            org.apache.calcite.sql.SqlFunctionCategory.SYSTEM), args.toArray(new SqlNode[0]), pos(ctx.start));
    }
  }

  /* ======================= Literals ======================= */

  @Override
  public SqlNode visitLiteralExpression(GaussDBSqlParser.LiteralExpressionContext ctx) {
    return visit(ctx.literal());
  }

  @Override
  public SqlNode visitIntegerLiteral(GaussDBSqlParser.IntegerLiteralContext ctx) {
    boolean negative = ctx.MINUS() != null;
    String text = (negative ? "-" : "") + ctx.INTEGER_VALUE().getText();
    return SqlNumericLiteral.createExactNumeric(text, pos(ctx.start));
  }

  @Override
  public SqlNode visitDecimalLiteral(GaussDBSqlParser.DecimalLiteralContext ctx) {
    boolean negative = ctx.MINUS() != null;
    String text = (negative ? "-" : "") + ctx.DECIMAL_VALUE().getText();
    return SqlNumericLiteral.createExactNumeric(text, pos(ctx.start));
  }

  @Override
  public SqlNode visitStringLit(GaussDBSqlParser.StringLitContext ctx) {
    return visit(ctx.stringLiteral());
  }

  @Override
  public SqlNode visitStringLiteral(GaussDBSqlParser.StringLiteralContext ctx) {
    String raw = ctx.STRING().getText();
    // Strip surrounding quotes and un-escape doubled quotes.
    String inner = raw.substring(1, raw.length() - 1).replace("''", "'");
    return SqlLiteral.createCharString(inner, pos(ctx.start));
  }

  @Override
  public SqlNode visitBooleanLit(GaussDBSqlParser.BooleanLitContext ctx) {
    return visit(ctx.booleanLiteral());
  }

  @Override
  public SqlNode visitBooleanLiteral(GaussDBSqlParser.BooleanLiteralContext ctx) {
    return SqlLiteral.createBoolean(ctx.TRUE() != null, pos(ctx.start));
  }

  @Override
  public SqlNode visitNullLit(GaussDBSqlParser.NullLitContext ctx) {
    return SqlLiteral.createNull(pos(ctx.start));
  }

  @Override
  public SqlNode visitNumberLit(GaussDBSqlParser.NumberLitContext ctx) {
    return visit(ctx.numberLiteral());
  }

  /* ======================= Helpers ======================= */

  /**
   * Emits a late-resolved function call whose name (and semantics) come from
   * the Hive / Dali registry via the chained operator table. Used for
   * GaussDB-to-Spark renames where we don't have a direct Calcite std op
   * (e.g. {@code decode}, {@code instr}, {@code regexp_extract}).
   */
  private static SqlNode emitUnresolved(String functionName, SqlParserPos pos, SqlNode[] args) {
    SqlIdentifier fnId = new SqlIdentifier(functionName, pos);
    return new SqlBasicCall(new org.apache.calcite.sql.SqlUnresolvedFunction(fnId, null, null, null, null,
        org.apache.calcite.sql.SqlFunctionCategory.SYSTEM), args, pos);
  }

  /**
   * {@code decode(sel, k1, v1, [k2, v2, ...], [default])} →
   * nested {@code CASE WHEN sel = k1 THEN v1 WHEN sel = k2 THEN v2 ... ELSE default END}.
   * Trailing odd operand (if any) is the else-branch per GaussDB/Oracle semantics.
   */
  private SqlNode buildDecode(GaussDBSqlParser.FunctionCallContext ctx, List<SqlNode> args) {
    if (args.size() < 3) {
      throw new UnhandledASTNodeException(ctx, "decode() needs at least 3 arguments");
    }
    SqlNode selector = args.get(0);
    int n = args.size();
    boolean hasElse = ((n - 1) % 2) == 1;
    int pairCount = (n - 1) / 2;
    List<SqlNode> whenList = new ArrayList<>(pairCount);
    List<SqlNode> thenList = new ArrayList<>(pairCount);
    for (int i = 0; i < pairCount; i++) {
      SqlNode key = args.get(1 + 2 * i);
      SqlNode value = args.get(2 + 2 * i);
      whenList.add(SqlStdOperatorTable.EQUALS.createCall(POS, selector, key));
      thenList.add(value);
    }
    SqlNode elseExpr = hasElse ? args.get(n - 1) : SqlLiteral.createNull(POS);
    return new SqlCase(pos(ctx.start), /*operand*/ null, new SqlNodeList(whenList, POS),
        new SqlNodeList(thenList, POS), elseExpr);
  }

  /**
   * Replaces the 2nd (format) argument of a GaussDB date/time conversion
   * function with its Spark-equivalent format string (if it is a literal).
   * Non-literal formats are left as-is — users get a clear "passthrough"
   * surface rather than a wrong-looking format string.
   */
  private SqlNode[] translateDateFormatArgs(List<SqlNode> args) {
    SqlNode[] result = args.toArray(new SqlNode[0]);
    if (result.length >= 2 && result[1] instanceof SqlLiteral
        && ((SqlLiteral) result[1]).getTypeName() == SqlTypeName.CHAR) {
      String pg = ((SqlLiteral) result[1]).getValueAs(String.class);
      String spark = translatePgDateFormat(pg);
      if (!spark.equals(pg)) {
        result[1] = SqlLiteral.createCharString(spark, SqlParserPos.ZERO);
      }
    }
    return result;
  }

  /**
   * PostgreSQL / GaussDB → Spark (Java DateTimeFormatter) format-token
   * rewrite. Only the tokens below are translated; everything else is kept
   * verbatim (case-preserving). This intentionally mirrors the plan §6
   * "format-token translator" requirement without pulling in a full
   * grammar — the v1 set covers the most common forms.
   *
   * <p>Public so tools / tests outside the package can reuse the mapping
   * (e.g. a Spark UDF wrapper that accepts a PG format string).
   */
  public static String translatePgDateFormat(String pg) {
    if (pg == null) {
      return null;
    }
    StringBuilder out = new StringBuilder(pg.length());
    int i = 0;
    while (i < pg.length()) {
      // Longest-match first; case-insensitive match on PG tokens.
      String rest = pg.substring(i);
      String upper = rest.toUpperCase();
      String replaced = null;
      int consumed = 0;
      if (upper.startsWith("YYYY")) { replaced = "yyyy"; consumed = 4; }
      else if (upper.startsWith("YY")) { replaced = "yy"; consumed = 2; }
      else if (upper.startsWith("MON")) { replaced = "MMM"; consumed = 3; }
      else if (upper.startsWith("MI")) { replaced = "mm"; consumed = 2; }
      else if (upper.startsWith("MM")) { replaced = "MM"; consumed = 2; }
      else if (upper.startsWith("DY")) { replaced = "EEE"; consumed = 2; }
      else if (upper.startsWith("DD")) { replaced = "dd"; consumed = 2; }
      else if (upper.startsWith("HH24")) { replaced = "HH"; consumed = 4; }
      else if (upper.startsWith("HH12")) { replaced = "hh"; consumed = 4; }
      else if (upper.startsWith("HH")) { replaced = "hh"; consumed = 2; }
      else if (upper.startsWith("SS")) { replaced = "ss"; consumed = 2; }
      else if (upper.startsWith("AM") || upper.startsWith("PM")) { replaced = "a"; consumed = 2; }
      else if (upper.startsWith("FF")) {
        // FF1..FF9 → S..SSSSSSSSS (sub-second precision).
        if (upper.length() > 2 && Character.isDigit(upper.charAt(2))) {
          int digits = upper.charAt(2) - '0';
          StringBuilder ss = new StringBuilder();
          for (int k = 0; k < digits; k++) ss.append('S');
          replaced = ss.toString();
          consumed = 3;
        } else {
          replaced = "SSS"; // FF with no digit → default 3 (ms).
          consumed = 2;
        }
      }
      if (replaced != null) {
        out.append(replaced);
        i += consumed;
      } else {
        out.append(pg.charAt(i));
        i++;
      }
    }
    return out.toString();
  }

  /** Heuristic: does the format contain any date/time token? */
  private static boolean containsDateFormatToken(String fmt) {
    if (fmt == null) return false;
    String u = fmt.toUpperCase();
    return u.contains("YYYY") || u.contains("YY") || u.contains("MM")
        || u.contains("DD") || u.contains("HH") || u.contains("MI")
        || u.contains("SS") || u.contains("MON") || u.contains("DY")
        || u.contains("FF") || u.contains("AM") || u.contains("PM");
  }

  private static SqlOperator comparisonOp(int tokenType) {
    switch (tokenType) {
      case GaussDBSqlParser.EQ:
        return SqlStdOperatorTable.EQUALS;
      case GaussDBSqlParser.NEQ:
        return SqlStdOperatorTable.NOT_EQUALS;
      case GaussDBSqlParser.LT:
        return SqlStdOperatorTable.LESS_THAN;
      case GaussDBSqlParser.LTE:
        return SqlStdOperatorTable.LESS_THAN_OR_EQUAL;
      case GaussDBSqlParser.GT:
        return SqlStdOperatorTable.GREATER_THAN;
      case GaussDBSqlParser.GTE:
        return SqlStdOperatorTable.GREATER_THAN_OR_EQUAL;
      default:
        throw new IllegalStateException("unknown comparison token: " + tokenType);
    }
  }

  private SqlIdentifier buildQualifiedIdentifier(GaussDBSqlParser.QualifiedNameContext ctx) {
    List<String> parts = new ArrayList<>();
    for (GaussDBSqlParser.IdentifierContext id : ctx.identifier()) {
      parts.add(identifierText(id));
    }
    return new SqlIdentifier(parts, pos(ctx.start));
  }

  private static String identifierText(GaussDBSqlParser.IdentifierContext ctx) {
    if (ctx instanceof GaussDBSqlParser.QuotedIdentifierContext) {
      String raw = ((GaussDBSqlParser.QuotedIdentifierContext) ctx).QUOTED_IDENTIFIER().getText();
      return raw.substring(1, raw.length() - 1).replace("\"\"", "\"");
    }
    // PostgreSQL folds unquoted identifiers to lowercase.
    return ctx.getText().toLowerCase();
  }

  private static String unquoteIfNeeded(String text) {
    if (text.length() >= 2 && text.charAt(0) == '"' && text.charAt(text.length() - 1) == '"') {
      return text.substring(1, text.length() - 1).replace("\"\"", "\"");
    }
    return text.toLowerCase();
  }

  private static String extractFunctionName(GaussDBSqlParser.FunctionNameContext ctx) {
    List<GaussDBSqlParser.IdentifierContext> ids = ctx.identifier();
    // Use the last segment; schema-qualified function calls are unusual in Coral IR.
    return identifierText(ids.get(ids.size() - 1));
  }

  private static String canonicalFunctionName(String raw) {
    return raw == null ? null : raw.toLowerCase();
  }

  private SqlNode buildTypeSpec(GaussDBSqlParser.TypeNameContext ctx) {
    String name = identifierText(ctx.baseTypeName().identifier()).toUpperCase();
    SqlTypeName sqlTypeName;
    switch (name) {
      case "INT":
      case "INT4":
      case "INTEGER":
        sqlTypeName = SqlTypeName.INTEGER;
        break;
      case "BIGINT":
      case "INT8":
        sqlTypeName = SqlTypeName.BIGINT;
        break;
      case "SMALLINT":
      case "INT2":
        sqlTypeName = SqlTypeName.SMALLINT;
        break;
      case "DOUBLE":
      case "FLOAT8":
      case "DOUBLE PRECISION":
        sqlTypeName = SqlTypeName.DOUBLE;
        break;
      case "REAL":
      case "FLOAT4":
        sqlTypeName = SqlTypeName.REAL;
        break;
      case "NUMERIC":
      case "DECIMAL":
        sqlTypeName = SqlTypeName.DECIMAL;
        break;
      case "VARCHAR":
      case "TEXT":
      case "CHARACTER VARYING":
        sqlTypeName = SqlTypeName.VARCHAR;
        break;
      case "CHAR":
      case "BPCHAR":
      case "CHARACTER":
        sqlTypeName = SqlTypeName.CHAR;
        break;
      case "BOOLEAN":
      case "BOOL":
        sqlTypeName = SqlTypeName.BOOLEAN;
        break;
      case "DATE":
        sqlTypeName = SqlTypeName.DATE;
        break;
      case "TIMESTAMP":
        sqlTypeName = SqlTypeName.TIMESTAMP;
        break;
      case "TIMESTAMPTZ":
      case "TIMESTAMP_WITH_TIME_ZONE":
        sqlTypeName = SqlTypeName.TIMESTAMP_WITH_LOCAL_TIME_ZONE;
        break;
      // GaussDB-specific / PG extension types mapped to Spark-friendly targets.
      // These lossy mappings keep the IR compilable; a future coral-spark
      // GaussDBCompatibilityTransformer (plan §6) can refine them.
      case "BYTEA":
        sqlTypeName = SqlTypeName.VARBINARY;
        break;
      case "UUID":
        // Spark has no native UUID; surface as VARCHAR(36) for lossless string form.
        return new org.apache.calcite.sql.SqlDataTypeSpec(
            new org.apache.calcite.sql.SqlBasicTypeNameSpec(SqlTypeName.VARCHAR, 36, -1, null, POS), POS);
      case "JSON":
      case "JSONB":
        // Same story: Spark uses string columns for JSON. VARCHAR is a safe stand-in.
        sqlTypeName = SqlTypeName.VARCHAR;
        break;
      case "INTERVAL":
        // INTERVAL without an explicit qualifier is ambiguous in Calcite; default
        // to DAY-TO-SECOND which covers most GaussDB usages. Users needing YEAR-MONTH
        // must currently cast through `CAST(x AS INTERVAL YEAR TO MONTH)` in Stage 4.
        sqlTypeName = SqlTypeName.INTERVAL_DAY_SECOND;
        break;
      default:
        throw new UnhandledASTNodeException(ctx, "unsupported GaussDB type: " + name);
    }

    // Parse optional precision/scale.
    if (ctx.LPAREN() != null) {
      List<TerminalNode> nums = ctx.INTEGER_VALUE();
      int precision = Integer.parseInt(nums.get(0).getText());
      int scale = nums.size() > 1 ? Integer.parseInt(nums.get(1).getText()) : -1;
      return new org.apache.calcite.sql.SqlDataTypeSpec(
          new org.apache.calcite.sql.SqlBasicTypeNameSpec(sqlTypeName, precision, scale, null, POS), POS);
    }
    return new org.apache.calcite.sql.SqlDataTypeSpec(new org.apache.calcite.sql.SqlBasicTypeNameSpec(sqlTypeName, POS),
        POS);
  }

  private static SqlParserPos pos(Token tok) {
    return new SqlParserPos(tok.getLine(), tok.getCharPositionInLine() + 1);
  }

  /** Fallback for any production we forgot to override. Per v1 policy: hard-fail. */
  @Override
  public SqlNode visitChildren(org.antlr.v4.runtime.tree.RuleNode node) {
    ParseTree pt = (ParseTree) node;
    if (pt instanceof ParserRuleContext) {
      throw new UnhandledASTNodeException(pt,
          "GaussDB parse-tree node not supported yet: " + pt.getClass().getSimpleName());
    }
    return super.visitChildren(node);
  }
}
