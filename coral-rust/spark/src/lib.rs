// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Spark-side helpers for coral-rust.
//!
//! Rust port of the two small Java modules:
//!
//!   - **`coral-spark-catalog`** — Java's `CoralSparkViewCatalog.loadView()`
//!     reaches into `SparkSession.active()` to register UDFs and parse the
//!     generated SQL. We can't replicate that without a live JVM, so the
//!     Rust port stops at producing a [`SparkView`] struct the caller hands
//!     to their own Spark driver — language-agnostic output, no session
//!     coupling. See [`view`] module.
//!
//!   - **`coral-spark-plan`** — Java's
//!     `SparkPlanToIRRelConverter.getComplicatedPredicatePushedDownInfo()`
//!     parses a plain-text Spark plan, walks the scan nodes, and classifies
//!     each pushed-down predicate as "simple" (=, >, IN, AND/OR) or
//!     "complicated" (contains a function call). No SparkSession needed —
//!     it's pure text analysis + sqlparser classification. Ported
//!     completely. See [`plan`] module.

pub mod error;
pub mod plan;
pub mod view;

pub use error::SparkError;
pub use plan::{
    analyze_plan, classify_predicate, PlanPredicateInfo, PredicateClass, PredicateFinding,
};
pub use view::{prepare_view, SparkView};
