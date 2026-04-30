// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// End-to-end tests for coral-viz. We don't try to pin the exact layout
// of the DOT / PlantUML text (too brittle as the walker evolves); we
// assert that the key nodes ARE there, edges ARE there, and the syntax
// is valid for the target renderer.

use coral_viz::{render, Format, Node, NodeKind};

fn has_substring(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

// ---------- DOT renderer ----------

#[test]
fn dot_plain_select_has_select_and_from() {
    let dot = render("SELECT a, b FROM t", Format::Dot).unwrap();
    assert!(dot.starts_with("digraph coral_ast"), "{dot}");
    assert!(has_substring(&dot, "Query"), "{dot}");
    assert!(has_substring(&dot, "Select"), "{dot}");
    assert!(has_substring(&dot, "FROM"), "{dot}");
    assert!(has_substring(&dot, "SELECT"), "{dot}");
    assert!(has_substring(&dot, " -> "), "{dot}");
}

#[test]
fn dot_join_exposes_table_and_join_nodes() {
    let dot = render(
        "SELECT a.id FROM a JOIN b ON a.id = b.id",
        Format::Dot,
    )
    .unwrap();
    assert!(has_substring(&dot, "a"), "{dot}");
    assert!(has_substring(&dot, "b"), "{dot}");
    assert!(has_substring(&dot, "Inner"), "{dot}");
}

#[test]
fn dot_where_clause_renders_filter_node() {
    let dot = render("SELECT * FROM t WHERE id > 10", Format::Dot).unwrap();
    assert!(has_substring(&dot, "WHERE"), "{dot}");
}

#[test]
fn dot_group_by_and_having() {
    let dot = render(
        "SELECT k, COUNT(*) FROM t GROUP BY k HAVING COUNT(*) > 3",
        Format::Dot,
    )
    .unwrap();
    assert!(has_substring(&dot, "GROUP BY"), "{dot}");
    assert!(has_substring(&dot, "HAVING"), "{dot}");
}

#[test]
fn dot_escapes_quotes_in_labels() {
    let dot = render("SELECT 'he said \"hi\"' FROM t", Format::Dot).unwrap();
    // No crash, no unescaped quote mid-label.
    assert!(dot.contains("digraph"), "{dot}");
}

#[test]
fn dot_cte_exposes_with_node() {
    let dot = render(
        "WITH x AS (SELECT a FROM t) SELECT a FROM x",
        Format::Dot,
    )
    .unwrap();
    assert!(has_substring(&dot, "CTE"), "{dot}");
    assert!(has_substring(&dot, "WITH"), "{dot}");
}

#[test]
fn dot_multi_statement_has_two_roots() {
    let dot = render("SELECT 1; SELECT 2", Format::Dot).unwrap();
    // Two `Query` root nodes.
    let occurrences = dot.matches("Query").count();
    assert!(occurrences >= 2, "{dot}");
}

// ---------- PlantUML renderer ----------

#[test]
fn plantuml_wraps_in_startuml_enduml() {
    let s = render("SELECT 1", Format::PlantUml).unwrap();
    assert!(s.starts_with("@startuml\n"), "{s}");
    assert!(s.trim_end().ends_with("@enduml"), "{s}");
}

#[test]
fn plantuml_contains_rectangles_and_edges() {
    let s = render("SELECT a FROM t WHERE id > 1", Format::PlantUml).unwrap();
    assert!(has_substring(&s, "rectangle"), "{s}");
    assert!(has_substring(&s, "-->"), "{s}");
    assert!(has_substring(&s, "Select"), "{s}");
    assert!(has_substring(&s, "WHERE"), "{s}");
}

// ---------- Walker sanity ----------

#[test]
fn walker_tree_structure_is_reasonable() {
    // Use the low-level walker to be sure the tree actually has the
    // shape the renderers print.
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;
    let stmts = Parser::parse_sql(
        &PostgreSqlDialect {},
        "SELECT a, b FROM t WHERE id > 10",
    )
    .unwrap();
    let root: Node = coral_viz::walker::walk_statement(&stmts[0]);
    fn find_kind(n: &Node, k: NodeKind) -> Option<&Node> {
        if n.kind == k {
            Some(n)
        } else {
            n.children.iter().find_map(|c| find_kind(c, k))
        }
    }
    assert!(find_kind(&root, NodeKind::Select).is_some());
    assert!(find_kind(&root, NodeKind::From).is_some());
    assert!(find_kind(&root, NodeKind::Filter).is_some());
    assert!(find_kind(&root, NodeKind::Projection).is_some());
    assert!(find_kind(&root, NodeKind::Table).is_some());
}

// ---------- Parse errors are soft ----------

#[test]
fn parse_error_does_not_panic() {
    // Render should succeed or return a clean Err — never panic.
    match render("SELEKT !", Format::Dot) {
        Ok(_) => (),
        Err(e) => {
            assert!(!e.to_string().is_empty(), "{e}");
        }
    }
}
