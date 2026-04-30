// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! GaussDB / openGauss SQL -> Spark SQL / Trino SQL translator.
//!
//! Rust port of the translation core in `coral-gaussdb` +
//! `coral-gaussdb-spark` + `coral-trino`. The Java originals go through
//! Apache Calcite (SqlNode -> RelNode -> SqlNode). This crate takes a
//! lighter route: parse with `sqlparser-rs` into its AST, apply rewrite
//! passes directly on the AST, then use `Display` to emit the target
//! dialect's SQL.
//!
//! No query optimizer, no type coercion, no catalog lookup — just
//! structural translation. Sufficient for the translate-only use case
//! that `CoralGaussDBToSpark` / `HiveToTrinoConverter` serve in the Java
//! tree.

pub mod catalog;
pub mod date_format;
pub mod error;
pub mod format;
pub mod function_catalog;
pub mod preprocess;
pub mod rewrite;
pub mod target;
pub mod translator;

pub use catalog::{Catalog, InMemoryCatalog, ValidationIssue};
pub use error::{CoralError, Result};
pub use function_catalog::{Category, Disposition, FunctionEntry};
pub use target::Target;
pub use translator::{
    translate, translate_all, translate_all_to, translate_to, translate_to_trino,
    translate_to_with, translate_with_catalog, translate_with_catalog_to, unknown_functions,
    CatalogTranslation,
};
