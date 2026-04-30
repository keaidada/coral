// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! GaussDB / openGauss SQL -> Spark SQL translator.
//!
//! Rust port of the translation core in `coral-gaussdb` + `coral-gaussdb-spark`.
//! The Java original goes through Apache Calcite (SqlNode -> RelNode -> SqlNode).
//! This crate takes a lighter route: parse with `sqlparser-rs` into its AST,
//! apply rewrite passes directly on the AST, then use `Display` to emit Spark SQL.
//!
//! No query optimizer, no type coercion, no catalog lookup — just structural
//! translation. Sufficient for the translate-only use case that `CoralGaussDBToSpark`
//! serves in the Java tree.

pub mod catalog;
pub mod date_format;
pub mod error;
pub mod preprocess;
pub mod rewrite;
pub mod translator;

pub use catalog::{Catalog, InMemoryCatalog, ValidationIssue};
pub use error::{CoralError, Result};
pub use translator::{translate, translate_all, translate_with_catalog};
