/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb.parsetree;

import org.antlr.v4.runtime.BaseErrorListener;
import org.antlr.v4.runtime.CharStreams;
import org.antlr.v4.runtime.CommonTokenStream;
import org.antlr.v4.runtime.RecognitionException;
import org.antlr.v4.runtime.Recognizer;

import com.linkedin.coral.gaussdb.parser.GaussDBSqlLexer;
import com.linkedin.coral.gaussdb.parser.GaussDBSqlParser;


/**
 * Thin driver around the generated ANTLR4 lexer + parser. Mirrors
 * {@code TrinoParserDriver} in coral-trino: takes a SQL string, returns a parse
 * tree rooted at {@code root}.
 *
 * <p>Syntax errors are escalated to {@link GaussDBParseException} with line/column
 * info — per the v1 "hard-fail" decision we do not silently recover.
 */
public final class GaussDBParserDriver {

  private GaussDBParserDriver() {
  }

  public static GaussDBSqlParser.RootContext parse(String sql) {
    if (sql == null) {
      throw new IllegalArgumentException("sql cannot be null");
    }
    GaussDBSqlLexer lexer = new GaussDBSqlLexer(CharStreams.fromString(sql));
    lexer.removeErrorListeners();
    lexer.addErrorListener(ThrowingErrorListener.INSTANCE);

    GaussDBSqlParser parser = new GaussDBSqlParser(new CommonTokenStream(lexer));
    parser.removeErrorListeners();
    parser.addErrorListener(ThrowingErrorListener.INSTANCE);

    return parser.root();
  }

  private static final class ThrowingErrorListener extends BaseErrorListener {
    static final ThrowingErrorListener INSTANCE = new ThrowingErrorListener();

    @Override
    public void syntaxError(Recognizer<?, ?> recognizer, Object offendingSymbol, int line, int charPositionInLine,
        String msg, RecognitionException e) {
      throw new GaussDBParseException(
          String.format("GaussDB parse error at line %d:%d — %s", line, charPositionInLine, msg), e);
    }
  }

  /**
   * Raised when the GaussDB ANTLR grammar rejects its input. Distinct from
   * {@link UnhandledASTNodeException} — that one means the grammar accepted the
   * syntax but we have not written a visitor case for it yet.
   */
  public static class GaussDBParseException extends RuntimeException {
    public GaussDBParseException(String message, Throwable cause) {
      super(message, cause);
    }
  }
}
