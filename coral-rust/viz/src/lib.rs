// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! AST visualization for coral-rust.
//!
//! Rust port of Java `coral-visualization`'s RelNode → PlantUML
//! renderer. The Rust version works on the sqlparser-rs AST (we don't
//! have Calcite's RelNode), so the tree looks a bit different — but
//! the idea is the same: each node is a labelled box, children are
//! connected with arrows, and the output is a text format a client
//! can pipe to `dot -Tsvg` or `plantuml -tsvg`.
//!
//! # Output formats
//!
//! - [`Format::Dot`] — Graphviz DOT (`text/vnd.graphviz`).
//! - [`Format::PlantUml`] — PlantUML (`text/plain`).
//!
//! # Example
//!
//! ```
//! use coral_viz::{render, Format};
//! let dot = render("SELECT a, b FROM t WHERE id > 10", Format::Dot).unwrap();
//! assert!(dot.starts_with("digraph"));
//! assert!(dot.contains("Query"));
//! assert!(dot.contains("Select"));
//! ```

mod dot;
mod plantuml;
pub mod walker;

use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;
use thiserror::Error;

pub use walker::{Node, NodeKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Dot,
    PlantUml,
}

#[derive(Debug, Error)]
pub enum VizError {
    #[error("parse error: {0}")]
    Parse(#[from] sqlparser::parser::ParserError),
}

/// Render a SQL string as a DOT or PlantUML graph.
pub fn render(sql: &str, format: Format) -> Result<String, VizError> {
    let dialect = PostgreSqlDialect {};
    let stmts = Parser::parse_sql(&dialect, sql)?;
    let roots: Vec<Node> = stmts.iter().map(walker::walk_statement).collect();
    Ok(match format {
        Format::Dot => dot::render(&roots),
        Format::PlantUml => plantuml::render(&roots),
    })
}
