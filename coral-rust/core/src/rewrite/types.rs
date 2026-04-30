// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Type-level rewrites for GaussDB / openGauss -> Spark SQL.
//!
//! Visits every `DataType` node in the AST (inside CAST, CREATE TABLE columns,
//! function type arguments, etc.) and rewrites GaussDB-specific types into
//! their closest Spark equivalent:
//!
//! | GaussDB type          | Spark type | Notes                              |
//! |-----------------------|------------|------------------------------------|
//! | `JSON` / `JSONB`      | `STRING`   | Spark has no native JSON type      |
//! | `UUID`                | `STRING`   | UUIDs stored as 36-char strings    |
//! | `BYTEA`               | `BINARY`   |                                    |
//! | `TIMESTAMP WITH TZ`   | `TIMESTAMP`| Spark lacks explicit TZ; assume UTC|
//! | `REGCLASS`            | `STRING`   | Catalog object reference           |
//! | `TEXT`                | `STRING`   | Spark canonical name               |
//! | Custom `SMALLSERIAL`  | `SMALLINT` | Stripped of sequence semantics     |
//! | Custom `SERIAL`       | `INT`      |                                    |
//! | Custom `BIGSERIAL`    | `BIGINT`   |                                    |
//! | `INTERVAL`            | `INTERVAL` | Preserved — Spark 3+ supports it   |
//!
//! The pass only touches types it recognizes; everything else (INT / VARCHAR /
//! DECIMAL / ARRAY / MAP / …) falls through unchanged. `Custom(name, args)` is
//! matched case-insensitively against a small known-set of GaussDB extensions.

use sqlparser::ast::{DataType, ObjectName, TimezoneInfo, VisitorMut};
use std::ops::ControlFlow;

pub struct TypeRewriter;

impl VisitorMut for TypeRewriter {
    type Break = std::convert::Infallible;

    fn post_visit_expr(&mut self, expr: &mut sqlparser::ast::Expr) -> ControlFlow<Self::Break> {
        // Types reach us via CAST expressions. The visitor derive on DataType
        // means pre/post_visit hooks for DataType exist too — but they aren't
        // exposed on `VisitorMut` in sqlparser 0.52. Walking via Cast
        // expressions covers the common case (`x::T`, `CAST(x AS T)`).
        if let sqlparser::ast::Expr::Cast { data_type, .. } = expr {
            rewrite_type(data_type);
        }
        ControlFlow::Continue(())
    }

    fn post_visit_statement(
        &mut self,
        stmt: &mut sqlparser::ast::Statement,
    ) -> ControlFlow<Self::Break> {
        // CREATE TABLE columns also carry DataTypes. Walk them explicitly.
        if let sqlparser::ast::Statement::CreateTable(ct) = stmt {
            for col in ct.columns.iter_mut() {
                rewrite_type(&mut col.data_type);
            }
        }
        ControlFlow::Continue(())
    }
}

/// In-place DataType rewrite. Public because Stage-3 rewrites (like Oracle
/// `(+)` outer joins) may also need to normalize types in intermediate rewrites.
pub fn rewrite_type(ty: &mut DataType) {
    match ty {
        // GaussDB JSON / JSONB aren't supported by Spark — fold to STRING.
        DataType::JSON | DataType::JSONB => {
            *ty = spark_string();
        }
        // UUID: no native Spark type.
        DataType::Uuid => {
            *ty = spark_string();
        }
        // BYTEA -> BINARY (Spark has Binary but not Bytea).
        DataType::Bytea => {
            *ty = DataType::Binary(None);
        }
        // TIMESTAMP WITH TIME ZONE / TIMESTAMPTZ -> plain TIMESTAMP. Spark
        // stores timestamps in UTC internally; the TZ suffix confuses Spark's
        // parser so we drop it.
        DataType::Timestamp(_precision, tz) if *tz != TimezoneInfo::None => {
            // Preserve precision if present (Spark ignores it, but keeping it
            // doesn't hurt readability).
            *tz = TimezoneInfo::None;
        }
        DataType::Regclass => {
            *ty = spark_string();
        }
        // Spark writes TEXT as STRING; literal "TEXT" is valid in Spark as a
        // synonym but canonicalizing helps downstream tools.
        DataType::Text => {
            *ty = spark_string();
        }
        // PG numeric aliases — sqlparser-rs parses these as dedicated variants
        // (not Custom). Spark has no Int2/Int4/Int8/Float4/Float8 spelling;
        // map to the canonical Spark name.
        DataType::Int2(_) => *ty = DataType::SmallInt(None),
        DataType::Int4(_) => *ty = DataType::Int(None),
        DataType::Int8(_) => *ty = DataType::BigInt(None),
        DataType::Float4 => *ty = DataType::Real,
        DataType::Float8 => *ty = DataType::Double,
        // Custom types: GaussDB has several Oracle-ish aliases that
        // sqlparser-rs doesn't know natively.
        DataType::Custom(name, _modifiers) => {
            let lower = object_name_last_lower(name);
            match lower.as_str() {
                "text" => *ty = spark_string(),
                "uuid" => *ty = spark_string(),
                "bytea" => *ty = DataType::Binary(None),
                "json" | "jsonb" => *ty = spark_string(),
                // SERIAL family -> corresponding integer type. GaussDB uses
                // these as auto-increment columns; Spark has no sequence
                // semantics so we just take the underlying integer width.
                "smallserial" | "serial2" => *ty = DataType::SmallInt(None),
                "serial" | "serial4" => *ty = DataType::Int(None),
                "bigserial" | "serial8" => *ty = DataType::BigInt(None),
                // GaussDB-specific numeric aliases.
                "int8" => *ty = DataType::BigInt(None),
                "int4" => *ty = DataType::Int(None),
                "int2" => *ty = DataType::SmallInt(None),
                "float8" => *ty = DataType::Double,
                "float4" => *ty = DataType::Real,
                "timestamptz" => {
                    *ty = DataType::Timestamp(None, TimezoneInfo::None);
                }
                _ => {
                    // Unknown custom — leave as-is. Spark's parser will either
                    // accept it verbatim or surface a clear error.
                }
            }
        }
        _ => {}
    }
}

/// Spark SQL's `STRING` is spelled as `DataType::Text` in sqlparser-rs (there's
/// no `STRING` variant). Using `Custom("STRING")` keeps the display output
/// literally `STRING`, matching what Spark emits natively.
fn spark_string() -> DataType {
    DataType::Custom(
        ObjectName(vec![sqlparser::ast::Ident::new("STRING")]),
        vec![],
    )
}

fn object_name_last_lower(name: &ObjectName) -> String {
    name.0
        .last()
        .map(|i| i.value.to_lowercase())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;

    fn parse_cast(sql: &str) -> DataType {
        // SELECT CAST(NULL AS <ty>) lets us probe the type visitor.
        let stmts = Parser::parse_sql(&PostgreSqlDialect {}, sql).unwrap();
        // Walk to find the Cast's data_type.
        let sqlparser::ast::Statement::Query(q) = &stmts[0] else {
            panic!("not a query");
        };
        let sqlparser::ast::SetExpr::Select(sel) = &*q.body else {
            panic!();
        };
        let sqlparser::ast::SelectItem::UnnamedExpr(e) = &sel.projection[0] else {
            panic!();
        };
        let sqlparser::ast::Expr::Cast { data_type, .. } = e else {
            panic!();
        };
        data_type.clone()
    }

    fn round_trip(sql: &str) -> String {
        crate::translate(sql).unwrap()
    }

    #[test]
    fn json_to_string() {
        let got = round_trip("SELECT x::JSON FROM t");
        assert!(got.contains("CAST(x AS STRING)"), "got: {got}");
    }

    #[test]
    fn jsonb_to_string() {
        let got = round_trip("SELECT x::JSONB FROM t");
        assert!(got.contains("CAST(x AS STRING)"), "got: {got}");
    }

    #[test]
    fn uuid_to_string() {
        let got = round_trip("SELECT x::UUID FROM t");
        assert!(got.contains("CAST(x AS STRING)"), "got: {got}");
    }

    #[test]
    fn bytea_to_binary() {
        let got = round_trip("SELECT x::BYTEA FROM t");
        assert!(got.contains("CAST(x AS BINARY)"), "got: {got}");
    }

    #[test]
    fn text_to_string() {
        let got = round_trip("SELECT x::TEXT FROM t");
        assert!(got.contains("CAST(x AS STRING)"), "got: {got}");
    }

    #[test]
    fn int4_custom_to_int() {
        // INT4 isn't a reserved word; sqlparser parses it as Custom.
        let got = round_trip("SELECT x::INT4 FROM t");
        assert!(got.contains("CAST(x AS INT)"), "got: {got}");
    }

    #[test]
    fn timestamptz_strips_timezone() {
        // TIMESTAMPTZ appears via Custom in sqlparser-rs (not recognized).
        let got = round_trip("SELECT x::TIMESTAMPTZ FROM t");
        assert!(got.contains("CAST(x AS TIMESTAMP)"), "got: {got}");
    }

    #[test]
    fn normal_types_untouched() {
        let got = round_trip("SELECT x::INT FROM t");
        assert!(got.contains("CAST(x AS INT)"), "got: {got}");
        let got = round_trip("SELECT x::BIGINT FROM t");
        assert!(got.contains("CAST(x AS BIGINT)"), "got: {got}");
        let got = round_trip("SELECT x::VARCHAR(10) FROM t");
        assert!(got.contains("CAST(x AS VARCHAR(10))"), "got: {got}");
    }

    #[test]
    fn direct_rewrite_api_json() {
        let mut ty = parse_cast("SELECT x::JSON FROM t");
        rewrite_type(&mut ty);
        assert_eq!(ty.to_string(), "STRING");
    }

    #[test]
    fn direct_rewrite_api_bytea() {
        let mut ty = parse_cast("SELECT x::BYTEA FROM t");
        rewrite_type(&mut ty);
        assert_eq!(ty.to_string(), "BINARY");
    }
}
