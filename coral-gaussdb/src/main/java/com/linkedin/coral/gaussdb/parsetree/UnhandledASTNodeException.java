/**
 * Copyright 2024-2026 LinkedIn Corporation. All rights reserved.
 * Licensed under the BSD-2 Clause license.
 * See LICENSE in the project root for license information.
 */
package com.linkedin.coral.gaussdb.parsetree;

import org.antlr.v4.runtime.ParserRuleContext;
import org.antlr.v4.runtime.Token;
import org.antlr.v4.runtime.tree.ParseTree;
import org.antlr.v4.runtime.tree.TerminalNode;


/**
 * Thrown when {@code ParseTreeBuilder} visits a GaussDB parse-tree node for which no
 * translation to a Calcite {@code SqlNode} has been implemented yet.
 *
 * <p>Per the v1 design decision (see GAUSSDB_DESIGN / plan: "未知语法/函数硬失败"), we
 * fail fast with an explicit error that identifies the input location and the raw
 * offending text so users can triage and contribute a mapping.
 */
public class UnhandledASTNodeException extends RuntimeException {

  public UnhandledASTNodeException(ParseTree node, String message) {
    super(String.format("%s — at %s, text=%s", message, locationOf(node), textOf(node)));
  }

  private static String locationOf(ParseTree node) {
    Token start = startTokenOf(node);
    if (start == null) {
      return "line ?:?";
    }
    return "line " + start.getLine() + ":" + start.getCharPositionInLine();
  }

  private static String textOf(ParseTree node) {
    String text = node.getText();
    if (text == null) {
      return "<null>";
    }
    // Truncate to keep error messages readable.
    return text.length() > 120 ? text.substring(0, 117) + "..." : text;
  }

  private static Token startTokenOf(ParseTree node) {
    if (node instanceof ParserRuleContext) {
      return ((ParserRuleContext) node).getStart();
    }
    if (node instanceof TerminalNode) {
      return ((TerminalNode) node).getSymbol();
    }
    return null;
  }
}
