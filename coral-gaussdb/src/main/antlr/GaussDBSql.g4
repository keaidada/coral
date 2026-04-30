/*
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 *
 * GaussDBSql.g4 — ANTLR4 grammar for the Coral coral-gaussdb frontend.
 *
 * Scope (v1 / Stage 1 MVP):
 *   - SELECT statements: projection list, FROM (table + alias + inner/left/right JOIN ON),
 *     WHERE, ORDER BY, LIMIT/OFFSET
 *   - Expressions: arithmetic, comparison, logical (AND/OR/NOT), IS [NOT] NULL,
 *     BETWEEN, IN (literal list), CASE WHEN, function call, parenthesized,
 *     CAST(expr AS type) AND x::type (PG double-colon cast), column references,
 *     string concat ||
 *   - Literals: integer, decimal, string (single-quoted), boolean (TRUE/FALSE), NULL
 *   - Identifiers: unquoted (folded by ParseTreeBuilder) and double-quoted (case-preserving)
 *
 * Out of scope (error / unsupported): DDL, GRANT/REVOKE, PL/pgSQL procedural blocks,
 * COPY, cursors, LOCK, row-level security, window frames, CTE (Stage 2), subqueries
 * (Stage 2), GROUP BY/HAVING/aggregation (Stage 2), MERGE/CONNECT BY (Stage 3).
 *
 * Reference: openGauss gram.y at db/openGauss-server/src/common/backend/parser/gram.y
 * We intentionally port only a minimal, test-driven subset. Every expansion must
 * ship with a matching golden test.
 */
grammar GaussDBSql;

/* ===================== Entry points ===================== */

root
    : statement SEMI? EOF
    ;

statement
    : selectStatement
    | insertStatement
    | deleteStatement
    | updateStatement
    | mergeStatement
    ;

insertStatement
    : INSERT INTO qualifiedName (LPAREN columns+=identifier (COMMA columns+=identifier)* RPAREN)?
      selectStatement
    ;

deleteStatement
    : DELETE FROM qualifiedName (AS? alias=identifier)? whereClause?
    ;

updateStatement
    : UPDATE qualifiedName (AS? alias=identifier)?
      SET assignment (COMMA assignment)*
      whereClause?
    ;

assignment
    : target=identifier EQ value=expression
    ;

// GaussDB MERGE INTO. v1 supports at most one WHEN MATCHED and one WHEN NOT
// MATCHED clause (the common case); multi-clause matching would need a
// SqlBasicVisitor extension per coral-common, so it is deferred.
mergeStatement
    : MERGE INTO target=qualifiedName (AS? targetAlias=identifier)?
      USING source=mergeSource (AS? sourceAlias=identifier)?
      ON cond=expression
      mergeWhen+
    ;

mergeSource
    : qualifiedName
    | LPAREN queryExpression RPAREN
    ;

mergeWhen
    : WHEN MATCHED THEN UPDATE SET assignment (COMMA assignment)*    # whenMatchedUpdate
    | WHEN NOT MATCHED THEN INSERT
      (LPAREN insertCols+=identifier (COMMA insertCols+=identifier)* RPAREN)?
      VALUES LPAREN insertVals+=expression (COMMA insertVals+=expression)* RPAREN  # whenNotMatchedInsert
    ;

/* ===================== SELECT ===================== */

selectStatement
    : withClause? queryExpression (ORDER BY sortItem (COMMA sortItem)*)?
      (LIMIT limit=numberLiteral)?
      (OFFSET offset=numberLiteral)?
    ;

// Non-recursive WITH only (Stage 2). Recursive CTE is scheduled for Stage 3
// together with CONNECT BY rewriting.
withClause
    : WITH namedQuery (COMMA namedQuery)*
    ;

namedQuery
    : name=identifier (LPAREN columnAliases+=identifier (COMMA columnAliases+=identifier)* RPAREN)?
      AS LPAREN queryExpression RPAREN
    ;

// Set operations — UNION/INTERSECT/EXCEPT. Left-associative, per SQL standard:
// a UNION b UNION c = (a UNION b) UNION c. Precedence: INTERSECT binds tighter
// than UNION/EXCEPT, so we model two levels.
queryExpression
    : queryTerm                                                          # queryTermDefault
    | left=queryExpression op=(UNION | EXCEPT | MINUS_KW) setQuantifier? right=queryTerm  # setOpUnionExcept
    ;

queryTerm
    : queryPrimary                                                       # queryPrimaryDefault
    | left=queryTerm INTERSECT setQuantifier? right=queryPrimary         # setOpIntersect
    ;

queryPrimary
    : select_                                                            # queryPrimarySelect
    | valuesClause                                                       # queryPrimaryValues
    | LPAREN queryExpression RPAREN                                      # queryPrimaryParens
    ;

valuesClause
    : VALUES valuesRow (COMMA valuesRow)*
    ;

valuesRow
    : LPAREN expression (COMMA expression)* RPAREN
    ;

select_
    : SELECT setQuantifier? distinctOnClause? selectItem (COMMA selectItem)*
      fromClause?
      whereClause?
      startWithClause?
      connectByClause?
      groupByClause?
      havingClause?
    ;

distinctOnClause
    : DISTINCT ON LPAREN expression (COMMA expression)* RPAREN
    ;

startWithClause
    : START WITH expression
    ;

connectByClause
    : CONNECT BY NOCYCLE? expression
    ;

groupByClause
    : GROUP BY expression (COMMA expression)*
    ;

havingClause
    : HAVING expression
    ;

// fromClause accepts one or more comma-joined sources; each source is a joinChain.
fromClause
    : FROM relation (COMMA relation)*
    ;

relation
    : left=tableRef joinClause*
    ;

joinClause
    : CROSS JOIN tableRef                                         # crossJoin
    | joinType=(INNER | LEFT | RIGHT | FULL) OUTER? JOIN tableRef joinCriteria  # qualifiedJoin
    | JOIN tableRef joinCriteria                                  # defaultInnerJoin
    ;

joinCriteria
    : ON expression                                               # joinOn
    | USING LPAREN identifier (COMMA identifier)* RPAREN          # joinUsing
    ;

tableRef
    : qualifiedName (AS? alias=identifier)?
    ;

whereClause
    : WHERE expression
    ;

selectItem
    : STAR                                   # selectAll
    | qualifiedName DOT STAR                 # selectQualifiedAll
    | expression (AS? alias=identifier)?     # selectExpression
    ;

setQuantifier
    : ALL
    | DISTINCT
    ;

sortItem
    : expression ordering=(ASC | DESC)? (NULLS nullOrder=(FIRST | LAST))?
    ;

/* ===================== Expressions =====================
 *
 * Precedence (from low to high) — matches PostgreSQL §4.1.6:
 *   OR, AND, NOT, comparison (= <> < > <= >=),
 *   IS [NOT] NULL / BETWEEN / IN, ||, + -, * / %, unary +/-, ::cast, atom
 */

expression
    : expression OR expression                                  # orExpr
    | expression AND expression                                 # andExpr
    | NOT expression                                            # notExpr
    | predicate                                                 # predicateDefault
    ;

predicate
    : valueExpression comparisonOp valueExpression              # comparisonPredicate
    | valueExpression IS NOT? NULL                              # isNullPredicate
    | valueExpression NOT? BETWEEN valueExpression AND valueExpression # betweenPredicate
    | valueExpression NOT? IN LPAREN literalList RPAREN         # inListPredicate
    | valueExpression NOT? IN LPAREN queryExpression RPAREN     # inSubqueryPredicate
    | NOT? EXISTS LPAREN queryExpression RPAREN                 # existsPredicate
    | valueExpression regexOp valueExpression                   # regexPredicate
    | valueExpression                                           # valueExpressionDefault
    ;

regexOp
    : TILDE_STAR                         // ~*  case-insensitive regex match
    | BANG_TILDE_STAR                    // !~* case-insensitive negated regex
    | TILDE                              // ~   regex match
    | BANG_TILDE                         // !~  negated regex
    ;

valueExpression
    : valueExpression op=(STAR | SLASH | PERCENT) valueExpression   # arithmeticMulDiv
    | valueExpression op=(PLUS | MINUS) valueExpression             # arithmeticAddSub
    | valueExpression CONCAT_OP valueExpression                     # stringConcat
    | valueExpression DOUBLE_COLON typeName                         # pgCastExpression
    | op=(PLUS | MINUS) valueExpression                             # unarySign
    | primaryExpression                                             # primaryDefault
    ;

primaryExpression
    : literal                                                         # literalExpression
    | caseExpression                                                  # caseExpr
    | castExpression                                                  # standardCastExpr
    | functionCall                                                    # functionExpr
    | PRIOR qualifiedName                                             # priorColumnReference
    | qualifiedName outerJoinMark                                     # oracleOuterJoinColumnReference
    | qualifiedName                                                   # columnReference
    | LPAREN queryExpression RPAREN                                   # scalarSubquery
    | LPAREN expression RPAREN                                        # parenthesizedExpression
    ;

// Oracle/GaussDB legacy outer-join marker — col(+) means "this side is the
// outer". We parse it so users don't get cryptic syntax errors, then fail
// loudly with a clear ANSI-JOIN suggestion.
outerJoinMark
    : LPAREN PLUS RPAREN
    ;

caseExpression
    : CASE (operand=expression)? whenClause+ (ELSE elseExpr=expression)? END
    ;

whenClause
    : WHEN condition=expression THEN result=expression
    ;

castExpression
    : CAST LPAREN expression AS typeName RPAREN
    ;

functionCall
    : functionName LPAREN (setQuantifier? functionArg (COMMA functionArg)*)? RPAREN overClause?
    ;

overClause
    : OVER LPAREN
        (PARTITION BY partitionBy+=expression (COMMA partitionBy+=expression)*)?
        (ORDER BY sortItem (COMMA sortItem)*)?
        frameClause?
      RPAREN
    ;

frameClause
    : frameKind=(ROWS | RANGE) BETWEEN frameStart=frameBound AND frameEnd=frameBound  # frameBetween
    | frameKind=(ROWS | RANGE) frameStart=frameBound                                  # frameSingle
    ;

frameBound
    : UNBOUNDED PRECEDING                         # frameUnboundedPreceding
    | UNBOUNDED FOLLOWING                         # frameUnboundedFollowing
    | CURRENT ROW                                 # frameCurrentRow
    | numberLiteral PRECEDING                     # frameNumPreceding
    | numberLiteral FOLLOWING                     # frameNumFollowing
    ;

functionArg
    : expression
    | STAR                      // e.g. count(*)
    ;

functionName
    : identifier (DOT identifier)?
    ;

literalList
    : literal (COMMA literal)*
    ;

literal
    : numberLiteral              # numberLit
    | stringLiteral              # stringLit
    | booleanLiteral             # booleanLit
    | NULL                       # nullLit
    ;

numberLiteral
    : MINUS? INTEGER_VALUE       # integerLiteral
    | MINUS? DECIMAL_VALUE       # decimalLiteral
    ;

stringLiteral
    : STRING
    ;

booleanLiteral
    : TRUE
    | FALSE
    ;

comparisonOp
    : EQ
    | NEQ
    | LT
    | LTE
    | GT
    | GTE
    ;

/* ===================== Types ===================== */

typeName
    : baseTypeName (LPAREN INTEGER_VALUE (COMMA INTEGER_VALUE)? RPAREN)?
    ;

baseTypeName
    : identifier
    ;

/* ===================== Identifiers ===================== */

qualifiedName
    : identifier (DOT identifier)*
    ;

identifier
    : IDENTIFIER                 # unquotedIdentifier
    | QUOTED_IDENTIFIER          # quotedIdentifier
    ;

/* ===================== Lexer ===================== */

// Keywords — must precede IDENTIFIER. Case-insensitive via fragment rules below.
SELECT      : S E L E C T ;
INSERT      : I N S E R T ;
INTO        : I N T O ;
DELETE      : D E L E T E ;
UPDATE      : U P D A T E ;
SET         : S E T ;
MERGE       : M E R G E ;
MATCHED     : M A T C H E D ;
VALUES      : V A L U E S ;
START       : S T A R T ;
CONNECT     : C O N N E C T ;
NOCYCLE     : N O C Y C L E ;
PRIOR       : P R I O R ;
WITH        : W I T H ;
FROM        : F R O M ;
WHERE       : W H E R E ;
GROUP       : G R O U P ;
HAVING      : H A V I N G ;
UNION       : U N I O N ;
EXCEPT      : E X C E P T ;
INTERSECT   : I N T E R S E C T ;
MINUS_KW    : M I N U S ;
ORDER       : O R D E R ;
BY          : B Y ;
OVER        : O V E R ;
PARTITION   : P A R T I T I O N ;
ROWS        : R O W S ;
RANGE       : R A N G E ;
ROW         : R O W ;
CURRENT     : C U R R E N T ;
UNBOUNDED   : U N B O U N D E D ;
PRECEDING   : P R E C E D I N G ;
FOLLOWING   : F O L L O W I N G ;
ASC         : A S C ;
DESC        : D E S C ;
NULLS       : N U L L S ;
FIRST       : F I R S T ;
LAST        : L A S T ;
LIMIT       : L I M I T ;
OFFSET      : O F F S E T ;
AS          : A S ;
ON          : O N ;
INNER       : I N N E R ;
LEFT        : L E F T ;
RIGHT       : R I G H T ;
FULL        : F U L L ;
CROSS       : C R O S S ;
OUTER       : O U T E R ;
JOIN        : J O I N ;
USING       : U S I N G ;
AND         : A N D ;
OR          : O R ;
NOT         : N O T ;
IS          : I S ;
NULL        : N U L L ;
BETWEEN     : B E T W E E N ;
IN          : I N ;
EXISTS      : E X I S T S ;
CASE        : C A S E ;
WHEN        : W H E N ;
THEN        : T H E N ;
ELSE        : E L S E ;
END         : E N D ;
CAST        : C A S T ;
DISTINCT    : D I S T I N C T ;
ALL         : A L L ;
TRUE        : T R U E ;
FALSE       : F A L S E ;

// Punctuation / operators
LPAREN      : '(' ;
RPAREN      : ')' ;
COMMA       : ',' ;
SEMI        : ';' ;
DOT         : '.' ;
STAR        : '*' ;
SLASH       : '/' ;
PERCENT     : '%' ;
PLUS        : '+' ;
MINUS       : '-' ;
EQ          : '=' ;
NEQ         : '<>' | '!=' ;
LT          : '<' ;
LTE         : '<=' ;
GT          : '>' ;
GTE         : '>=' ;
CONCAT_OP   : '||' ;
DOUBLE_COLON: '::' ;
// Regex operators — PostgreSQL / GaussDB dialect. Order matters: the longer
// multi-char tokens (~*, !~*, !~) must precede the single-char TILDE so the
// lexer's greedy match picks the right alternative.
TILDE_STAR      : '~*' ;
BANG_TILDE_STAR : '!~*' ;
BANG_TILDE      : '!~' ;
TILDE           : '~' ;

// Literals
fragment DIGIT  : [0-9] ;
INTEGER_VALUE   : DIGIT+ ;
DECIMAL_VALUE   : DIGIT+ '.' DIGIT* | '.' DIGIT+ ;

// Single-quoted string; doubled '' escapes a single quote (SQL standard).
STRING
    : '\'' ( ~('\'') | '\'\'' )* '\''
    ;

// Unquoted identifier. Note: GaussDB/PG folds these to lowercase;
// the ParseTreeBuilder applies that normalization.
IDENTIFIER
    : [A-Za-z_] [A-Za-z0-9_]*
    ;

// Double-quoted identifier preserves case. "" escapes a literal quote.
QUOTED_IDENTIFIER
    : '"' ( ~('"') | '""' )* '"'
    ;

/* ===================== Whitespace & comments ===================== */

// GaussDB/Oracle-style optimizer hints. Parsed and discarded in v1 —
// forwarding them to Spark would require dialect-specific translation that
// we defer to a future milestone. HINT must precede BLOCK_COMMENT so the
// lexer prefers the more specific rule on `/*+ ... */`.
HINT          : '/*+' .*? '*/'          -> skip ;
LINE_COMMENT  : '--' ~[\r\n]*           -> skip ;
BLOCK_COMMENT : '/*' .*? '*/'           -> skip ;
WS            : [ \t\r\n]+              -> skip ;

/* ===================== Case-insensitive letter fragments ===================== */

fragment A : [aA] ; fragment B : [bB] ; fragment C : [cC] ; fragment D : [dD] ;
fragment E : [eE] ; fragment F : [fF] ; fragment G : [gG] ; fragment H : [hH] ;
fragment I : [iI] ; fragment J : [jJ] ; fragment K : [kK] ; fragment L : [lL] ;
fragment M : [mM] ; fragment N : [nN] ; fragment O : [oO] ; fragment P : [pP] ;
fragment Q : [qQ] ; fragment R : [rR] ; fragment S : [sS] ; fragment T : [tT] ;
fragment U : [uU] ; fragment V : [vV] ; fragment W : [wW] ; fragment X : [xX] ;
fragment Y : [yY] ; fragment Z : [zZ] ;
