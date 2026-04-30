// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Avro-schema inference for SQL views.
//!
//! Rust port of Java `coral-schema` — specifically the
//! `ViewToAvroSchemaConverter.toAvroSchema(db, table)` entry point that
//! LinkedIn uses to materialize an Avro schema from a view DDL so the
//! view can be stored alongside physical datasets.
//!
//! # What this crate does
//!
//! Given a `CREATE VIEW` statement (or any `SELECT`) plus a
//! [`coral_core::Catalog`] describing the physical tables, it produces
//! an Avro schema in JSON form that matches the view's output columns.
//!
//! # Trade-offs vs the Java implementation
//!
//! The Java tree does this work on a Calcite `RelNode` tree, which
//! carries full type information from the planner. The Rust port does
//! NOT run Calcite — so it infers types structurally from the AST +
//! catalog:
//!
//!   - Column references resolve through the catalog's `columns_of()`
//!     which returns `&[String]` of the form `"name|type"`.
//!   - Literals infer from the literal kind (integer literal → LONG,
//!     string literal → STRING, etc.).
//!   - Arithmetic operators pick the "wider" operand type (LONG + INT
//!     → LONG, DOUBLE + LONG → DOUBLE).
//!   - Aggregates fall back to nullable output types based on the
//!     function name (COUNT → LONG non-null, SUM/AVG → DOUBLE null,
//!     MIN/MAX → input type, …).
//!   - Function calls we don't recognize produce a nullable STRING
//!     (the most permissive Avro type) rather than failing.
//!
//! This covers the practical "view column → Avro field" mapping for
//! flat columnar projections and simple aggregations, which is the
//! bulk of what the Java converter handles. Deeply-nested struct
//! field access (`t.a.b.c` with user-defined record schemas) is out
//! of scope for this port — document that in the README.
//!
//! # Example
//!
//! ```no_run
//! use coral_core::InMemoryCatalog;
//! use coral_schema::to_avro_schema;
//!
//! let catalog = InMemoryCatalog::from_pairs(&[
//!     ("db", "employees", &["id|BIGINT", "name|VARCHAR", "salary|DOUBLE"]),
//! ]);
//! let json = to_avro_schema(
//!     "CREATE VIEW v AS SELECT id, name, salary FROM db.employees",
//!     &catalog,
//! ).unwrap();
//! assert!(json.contains("\"name\":\"id\""));
//! assert!(json.contains("\"type\":\"long\""));
//! ```

pub mod avro;
pub mod error;
pub mod infer;

pub use avro::{AvroField, AvroRecord, AvroType};
pub use error::SchemaError;
pub use infer::{to_avro_record, to_avro_schema, TypedCatalog};
