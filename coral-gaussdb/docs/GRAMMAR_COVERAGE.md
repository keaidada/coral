# GaussDB Grammar Coverage

Our `.g4` rules ↔ openGauss `gram.y` ↔ tests ↔ support state. Serves as the
single-source-of-truth for what coral-gaussdb parses today and where the
boundary lives.

Reference: `/db/openGauss-server/src/common/backend/parser/gram.y` (~35k lines).
**We do not port the full grammar — only the subset needed to translate the
representative GaussDB workloads to Spark SQL.**

## Legend

| Symbol | Meaning |
|---|---|
| ✅ | Fully supported and under test |
| 🟡 | Parsed; semantics limited (see notes) |
| ❌ | Not supported in v1; hard error with a clear message |

## Stage 1 — Foundation (M1)

| Our rule | gram.y section | Test | State |
|---|---|---|---|
| `selectStatement` | `SelectStmt` / `simple_select` | `testBasicSelectParsesCleanly`, `testEndToEndRelNodeFromSimpleSelect` | ✅ |
| `selectItem`, `STAR`, `qualifiedName DOT STAR` | `target_list` / `target_el` | `testBasicSelectParsesCleanly` | ✅ |
| `fromClause`, `relation`, `tableRef` (single table + alias) | `from_clause`, `table_ref`, `qualified_name` | `testBasicSelectParsesCleanly` | ✅ |
| `joinClause` INNER/LEFT/RIGHT with `ON`  | `join_type` `joined_table` | `testJoinsParse` | ✅ |
| `whereClause` | `where_clause` | `testBasicSelectParsesCleanly` | ✅ |
| `sortItem` ASC/DESC + NULLS FIRST/LAST | `sort_clause`, `opt_sort_clause` | (covered by end-to-end tests) | ✅ |
| `LIMIT` / `OFFSET` | `limit_clause`, `select_limit` | `testSimpleSelect` (spark) | ✅ |
| Arithmetic `+ - * / %` | `a_expr` arithmetic | implicit in end-to-end tests | ✅ |
| Comparison `= <> != < > <= >=` | `a_expr` comparison, `all_Op` | implicit | ✅ |
| Logical `AND / OR / NOT` | `a_expr` logical | implicit | ✅ |
| `IS [NOT] NULL`, `[NOT] BETWEEN`, `IN (literal-list)` | `a_expr`: `IS_NULL`, `BETWEEN`, `IN_P` | (embedded in test suite) | ✅ |
| `CASE WHEN` | `case_expr` | `testCaseAndNullFunctionsParse` | ✅ |
| Function call | `func_application`, `func_expr_common_subexpr` | pervasive | ✅ |
| Literals (int / numeric / string / boolean / NULL) | `AexprConst` | pervasive | ✅ |
| Unquoted / double-quoted identifiers (with case folding) | `ColId` / `ColLabel` / `QuotedColName` | implicit | ✅ |
| `CAST(x AS t)` / `x::t` | `TypecastExpression`, `func_expr` / `a_expr ::` | `testCastExpressionsParse`, `testPgCastRewrittenToStandardCast` | ✅ |
| String concat `\|\|` | `a_expr ||` | `testStringConcatRewrittenToConcat` | ✅ |

## Stage 2 — Standard DML (M2)

| Our rule | gram.y section | Test | State |
|---|---|---|---|
| `groupByClause` / `havingClause` | `group_clause`, `having_clause` | `testGroupByHavingEndToEnd`, `testGroupByAndHaving` | ✅ |
| `UNION [ALL]` / `INTERSECT [ALL]` / `EXCEPT [ALL]` / `MINUS` | `simple_select` set-op forms | `testUnionAllEndToEnd`, `testMinusIsExcept`, `testUnionAll` | ✅ |
| `withClause` (non-recursive), `namedQuery` with column aliases | `with_clause`, `common_table_expr` | `testSingleCteEndToEnd`, `testMultiCteWithColumnAlias`, `testNonRecursiveCte` | ✅ |
| Scalar subquery in expression position | `c_expr`: `LPAREN a_expr RPAREN` / `select_with_parens` | `testScalarSubqueryInProjection` | ✅ |
| `IN (subquery)` / `NOT IN (subquery)` | `a_expr IN_P select_with_parens` | `testInSubquery` | ✅ |
| `[NOT] EXISTS (subquery)` | `a_expr EXISTS_P select_with_parens` | `testCorrelatedExists` | ✅ |
| `FULL OUTER / CROSS` joins, `USING (col, ...)` | `joined_table` full/cross and `USING` | `testJoinVariants` | ✅ |
| Window functions `OVER (PARTITION BY ... ORDER BY ...)` | `over_clause`, `window_clause` | `testWindowRowNumber`, `testWindowFunction` | ✅ |
| Window frame (`ROWS BETWEEN …`, `RANGE …`) | `frame_clause` | — | ❌ Stage 4 |
| `DISTINCT`, `DISTINCT ON` | `opt_all_clause`, `distinct_clause`, `DISTINCT ON` | DISTINCT ✅ ; DISTINCT ON ❌ |
| `INSERT INTO t [cols] SELECT ...` | `insert_rest` | `testInsertSelect` | ✅ |
| `INSERT INTO t VALUES (...)` | `values_clause` | — | ❌ Stage 4 (MERGE uses it internally) |

## Stage 3 — Advanced & GaussDB-specific (M3)

| Our rule | gram.y section | Test | State |
|---|---|---|---|
| Optimizer hints `/*+ ... */` | `hint_gram.y` (openGauss-specific) | `testHintsAreDiscarded`, `testHintDiscarded` | 🟡 parsed and discarded |
| `DELETE FROM t [alias] [WHERE ...]` | `DeleteStmt` | `testDelete`, `testDeleteAllRows` | ✅ |
| `UPDATE t SET c=e, ... [WHERE ...]` | `UpdateStmt` | `testUpdate` | ✅ |
| `MERGE INTO ... USING ... ON ... WHEN MATCHED/NOT MATCHED ...` | `MergeStmt` / `parse_merge.cpp` | `testMergeIntoParses`, `testMergeRelConversionSurfaceDiagnostic`, `testMergeEndToEnd` | ✅ end-to-end via SqlNode unparse to Spark dialect (Spark 3.x MERGE is syntax-compatible) |
| `START WITH … CONNECT BY [PRIOR] …` | Recognized in gram.y "not yet supported" path; we rewrite in ParseTreeBuilder to a recursive CTE | `testConnectByRewritesToSqlWith`, `testConnectByEndToEnd` | ✅ end-to-end via SqlNode unparse (`WITH __coral_connect_by AS (anchor UNION ALL step)`) |
| `NOCYCLE`, `ORDER SIBLINGS BY`, `CONNECT_BY_ISCYCLE`, `SYS_CONNECT_BY_PATH` | gram.y CONNECT BY extensions | `testNoCycleRejected` | ❌ hard-fail (v1 limit) |
| GaussDB types `BYTEA`, `UUID`, `JSON`, `JSONB`, `TIMESTAMPTZ`, `INTERVAL` | `Typename` / `SimpleTypename` extensions | `testExtendedTypeCasts` | 🟡 lossy mapping to Spark-compatible std types |

## Stage 4 — Polish & extended syntax (S4)

| Our rule | gram.y section | Test | State |
|---|---|---|---|
| Regex operators `~`, `~*`, `!~`, `!~*` | `a_expr` POSIX regex | `testRegexOperators`, `testCaseInsensitiveRegexLowerFolds` | ✅ rewritten to `rlike` (case-insensitive folds with `LOWER`) |
| Window frame `ROWS / RANGE BETWEEN … AND …`, single-bound form, `UNBOUNDED PRECEDING/FOLLOWING`, `CURRENT ROW`, `N PRECEDING/FOLLOWING` | `frame_clause` | `testWindowFrameRowsBetween`, `testWindowFrameRangeUnbounded`, `testWindowFrameSingleBound` | ✅ |
| Standalone `VALUES (...)` statement | `values_clause` | `testStandaloneValues` | ✅ |
| `DISTINCT ON (expr, ...)` | `distinct_clause DISTINCT ON (..)` | `testDistinctOnRewritten`, `testDistinctOnWithoutOrderBy`, `testDistinctOnInsideSetOpRejected` | ✅ rewritten to ROW_NUMBER subquery with PARTITION BY on DISTINCT ON keys; rejected inside set-ops/subqueries where outer ORDER BY is not in scope |
| `to_date / to_char / to_timestamp` format-token translation (PG → Spark DateTimeFormatter) | — | `testPgDateFormatTranslator`, `testToDateFormatRewrite`, `testToCharRewritesToDateFormat` | ✅ tokens: YYYY YY MM MON DD DY HH HH12 HH24 MI SS FF<N> AM PM; `to_char(date,...)` → `date_format(...)` |

## Stage 5 — Advanced rewrites & end-to-end coverage (S5)

| Feature | Test | State |
|---|---|---|
| `generate_series(a, b [, step])` → `sequence(...)` | `testGenerateSeriesTwoArg`, `testGenerateSeriesThreeArg` | ✅ |
| Oracle legacy `(+)` outer-join marker | `testOracleLegacyOuterJoinRejected` | ❌ hard-fail with actionable error (use ANSI JOIN) |
| MERGE end-to-end to Spark SQL | `testMergeEndToEnd` | ✅ via SqlNode-level unparse fallback |
| Recursive CTE / CONNECT BY end-to-end | `testConnectByEndToEnd` | ✅ via SqlNode-level unparse fallback |
| DISTINCT ON rewrite | `testDistinctOnRewritten` | ✅ |

## Function rewrites (plan §6 mapping table)

Most GaussDB → Spark function differences collapse at AST time in
`ParseTreeBuilder.visitFunctionCall`. Direct std-op emits prevent the Calcite
validator from treating aggregates as UDFs.

| GaussDB | Translation | Layer |
|---|---|---|
| `sum/count/avg/min/max` | `SqlStdOperatorTable.*` direct | ParseTreeBuilder |
| `count(*)` | `COUNT(<star-ident>)` via `CalciteUtil.createStarIdentifier` | ParseTreeBuilder |
| `row_number / rank / dense_rank` | `SqlStdOperatorTable.*` direct | ParseTreeBuilder |
| `nvl(a,b)` | `COALESCE(a,b)` | ParseTreeBuilder |
| `nvl2(a,b,c)` | `CASE WHEN a IS NOT NULL THEN b ELSE c END` | ParseTreeBuilder |
| `decode(sel, k,v, ..., default)` | nested `CASE WHEN` | ParseTreeBuilder |
| `sysdate / now()` | `CURRENT_TIMESTAMP` | ParseTreeBuilder |
| `substr(s,p,l)` | `SUBSTRING(s,p,l)` | ParseTreeBuilder |
| `x::t`  → `CAST(x AS t)` | `SqlStdOperatorTable.CAST` | ParseTreeBuilder |
| `a \|\| b` | `CONCAT(a, b)` (Coral-spark may retranslate to `\|\|`) | ParseTreeBuilder |
| `mod(a,b)` | `a % b` (MOD op) | ParseTreeBuilder |
| `random()` | `rand()` | ParseTreeBuilder |
| `position(a, b)` | `instr(b, a)` (arg swap) | ParseTreeBuilder |
| `bool_and / bool_or` | `every / some` | ParseTreeBuilder |
| `array_agg(x)` | `collect_list(x)` | ParseTreeBuilder |
| `string_agg(x, sep)` | `concat_ws(sep, collect_list(x))` | ParseTreeBuilder |
| `trunc(date, unit)` | `date_trunc(unit, date)` (arg swap) | ParseTreeBuilder |
| `regexp_substr(s, p)` | `regexp_extract(s, p, 0)` | ParseTreeBuilder |
| `to_date(s, fmt)`, `to_char(x, fmt)` | — | **Backend TODO** (format-token translation — plan §6 Stage-4) |
| `generate_series(a, b)` | — | **Backend TODO** (→ `sequence(a,b)` as table-valued) |

## Known gaps (Stage 6+ punch-list)

1. **Numeric `to_char(num, '9.99')`** — v1 passes through unchanged; Spark's
   numeric format tokens differ materially and a safe mapping needs its own
   translator.
2. **Unicode case folding for `~*`** — current `LOWER()` approach is ASCII-only.
3. **GaussDB partition syntax** (`PARTITION BY`, `SUBPARTITION`) — not parsed.
4. **LATERAL joins / table-valued function joins** — partially reachable via
   `generate_series` but without `LATERAL` the rewrite quality is limited.
5. **MERGE rel-level optimization** — currently uses SqlNode-level unparse
   which means no rel-plan inspection / rewriting. Upstream Calcite would
   need `convertMerge` preconditions relaxed.
6. **`(+)` multi-operand expressions** — we reject on first encounter; users
   with complex legacy predicates need a dedicated rewrite helper (out of
   scope for a Spark target since ANSI JOIN is the only acceptable output).
7. **Oracle `CONNECT_BY_ISCYCLE`, `SYS_CONNECT_BY_PATH`** pseudo-functions —
   still unsupported in the CONNECT BY rewrite.
8. **Stored procedures, PL/pgSQL, cursors** — explicitly out of scope.
