// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Inference pipeline: SQL string → AST → Avro record.

use sqlparser::ast::{
    BinaryOperator, Expr, Ident, ObjectName, Query, Select, SelectItem, SetExpr, Statement,
    TableFactor, TableWithJoins, Value,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

use crate::avro::{AvroField, AvroRecord, AvroType};
use crate::error::{Result, SchemaError};

/// A catalog that knows both column names AND column types.
///
/// The base `coral_core::Catalog` trait only exposes column names, which
/// isn't enough for type inference. This trait layers on top with a
/// `column_type` query. Implementations for:
///
///   - [`coral_core::InMemoryCatalog`] (automatic impl below).
///   - User-written wrappers around remote metastore clients.
pub trait TypedCatalog {
    /// Return the Hive/SQL type string for `db.table.column`, or None
    /// if the column is unknown. Type strings follow the Hive syntax:
    /// `BIGINT`, `VARCHAR`, `ARRAY<INT>`, `MAP<STRING, DOUBLE>`,
    /// `DECIMAL(18,2)`, `STRUCT<f1:INT, f2:STRING>`, etc.
    fn column_type(&self, db: &str, table: &str, column: &str) -> Option<String>;

    /// List all columns for `db.table`, preserving declaration order.
    fn columns(&self, db: &str, table: &str) -> Option<Vec<String>>;
}

impl TypedCatalog for coral_core::InMemoryCatalog {
    fn column_type(&self, db: &str, table: &str, column: &str) -> Option<String> {
        let (cols, types) = self.column_types(db, table)?;
        cols.iter()
            .position(|c| c.eq_ignore_ascii_case(column))
            .and_then(|i| types.get(i).cloned())
    }

    fn columns(&self, db: &str, table: &str) -> Option<Vec<String>> {
        Some(self.columns(db, table)?.to_vec())
    }
}

/// Entry point: turn a SQL view definition into an Avro schema JSON.
///
/// Accepted inputs:
///   - `CREATE VIEW <n> AS SELECT ...`
///   - bare `SELECT ...`
///   - `CREATE [OR REPLACE] VIEW <n>(...) AS SELECT ...` (explicit columns)
pub fn to_avro_schema<C: TypedCatalog>(sql: &str, catalog: &C) -> Result<String> {
    let record = to_avro_record(sql, catalog)?;
    Ok(serde_json::to_string_pretty(&record)?)
}

/// Like [`to_avro_schema`] but returns the in-memory [`AvroRecord`]
/// so callers can inspect or manipulate the schema before rendering.
pub fn to_avro_record<C: TypedCatalog>(sql: &str, catalog: &C) -> Result<AvroRecord> {
    let dialect = PostgreSqlDialect {};
    let mut stmts = Parser::parse_sql(&dialect, sql)?;

    let (view_name, query) = extract_view(&mut stmts)?;
    let select = unwrap_select(&query)?;

    // Build scope: (alias, db, table) triples for the FROM clause.
    let scope = build_scope(&select.from, catalog);

    let mut fields = Vec::new();
    for (i, item) in select.projection.iter().enumerate() {
        let (name, ty) = project_field(item, i, &scope, catalog);
        fields.push(AvroField {
            name,
            avro_type: ty,
            doc: None,
        });
    }

    Ok(AvroRecord {
        name: view_name,
        namespace: None,
        fields,
    })
}

// -------------------------------------------------------------------
// Statement reshaping
// -------------------------------------------------------------------

fn extract_view(stmts: &mut [Statement]) -> Result<(String, Query)> {
    match stmts {
        [Statement::CreateView { name, query, .. }] => {
            Ok((name_last_ident(name), (**query).clone()))
        }
        [Statement::Query(q)] => Ok(("view".to_string(), (**q).clone())),
        _ => Err(SchemaError::NoSelect),
    }
}

fn unwrap_select(q: &Query) -> Result<Select> {
    match q.body.as_ref() {
        SetExpr::Select(s) => Ok(*s.clone()),
        SetExpr::SetOperation { left, .. } => {
            // Use the left branch of the set-op for column layout —
            // UNIONs require matching layouts anyway.
            let left_query = Query {
                body: left.clone(),
                with: None,
                order_by: None,
                limit: None,
                limit_by: vec![],
                offset: None,
                fetch: None,
                locks: vec![],
                for_clause: None,
                settings: None,
                format_clause: None,
            };
            unwrap_select(&left_query)
        }
        other => Err(SchemaError::Unsupported(format!("SELECT body: {other:?}"))),
    }
}

fn name_last_ident(n: &ObjectName) -> String {
    n.0.last().map(|i| i.value.clone()).unwrap_or_default()
}

// -------------------------------------------------------------------
// Scope resolution
// -------------------------------------------------------------------

#[derive(Debug, Clone)]
struct FromEntry {
    alias: String,
    db: String,
    table: String,
}

fn build_scope<C: TypedCatalog>(from: &[TableWithJoins], _catalog: &C) -> Vec<FromEntry> {
    let mut out = Vec::new();
    for twj in from {
        collect_table_factor(&twj.relation, &mut out);
        for j in &twj.joins {
            collect_table_factor(&j.relation, &mut out);
        }
    }
    out
}

fn collect_table_factor(factor: &TableFactor, out: &mut Vec<FromEntry>) {
    if let TableFactor::Table { name, alias, .. } = factor {
        let parts: Vec<String> = name.0.iter().map(|i| i.value.clone()).collect();
        let (db, table) = match parts.len() {
            1 => ("default".to_string(), parts[0].clone()),
            2 => (parts[0].clone(), parts[1].clone()),
            _ => (parts[0].clone(), parts[parts.len() - 1].clone()),
        };
        let alias_name = alias
            .as_ref()
            .map(|a| a.name.value.clone())
            .unwrap_or_else(|| table.clone());
        out.push(FromEntry {
            alias: alias_name,
            db,
            table,
        });
    }
}

// -------------------------------------------------------------------
// Projection → field
// -------------------------------------------------------------------

fn project_field<C: TypedCatalog>(
    item: &SelectItem,
    index: usize,
    scope: &[FromEntry],
    catalog: &C,
) -> (String, AvroType) {
    match item {
        SelectItem::UnnamedExpr(e) => {
            let ty = infer_type(e, scope, catalog);
            (infer_name(e).unwrap_or_else(|| format!("col{index}")), ty)
        }
        SelectItem::ExprWithAlias { expr, alias } => {
            (alias.value.clone(), infer_type(expr, scope, catalog))
        }
        SelectItem::Wildcard(_) | SelectItem::QualifiedWildcard(_, _) => {
            // Wildcards blow up the field count — we'd need to expand
            // them here. The practical view case rarely uses `*` in
            // the outermost projection; keep it as a single STRING
            // placeholder rather than failing. A future version can
            // refuse wildcards instead.
            (
                format!("col{index}"),
                AvroType::Nullable(Box::new(AvroType::String)),
            )
        }
    }
}

fn infer_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Identifier(Ident { value, .. }) => Some(value.clone()),
        Expr::CompoundIdentifier(parts) => parts.last().map(|i| i.value.clone()),
        _ => None,
    }
}

// -------------------------------------------------------------------
// Type inference
// -------------------------------------------------------------------

fn infer_type<C: TypedCatalog>(expr: &Expr, scope: &[FromEntry], catalog: &C) -> AvroType {
    nullable(infer_type_inner(expr, scope, catalog))
}

fn infer_type_inner<C: TypedCatalog>(expr: &Expr, scope: &[FromEntry], catalog: &C) -> AvroType {
    match expr {
        // ---- References ----
        Expr::Identifier(ident) => {
            lookup_bare_column(&ident.value, scope, catalog).unwrap_or(AvroType::String)
        }

        Expr::CompoundIdentifier(parts) => {
            if parts.len() == 2 {
                // qualifier.column
                let (q, c) = (&parts[0].value, &parts[1].value);
                lookup_qualified_column(q, c, scope, catalog).unwrap_or(AvroType::String)
            } else {
                AvroType::String
            }
        }

        // ---- Literals ----
        Expr::Value(v) => value_to_avro(v),

        // ---- Casts ----
        Expr::Cast { data_type, .. } => data_type_to_avro(data_type),

        // ---- Arithmetic / comparisons ----
        Expr::BinaryOp { left, op, right } => {
            let l = strip_nullable(infer_type_inner(left, scope, catalog));
            let r = strip_nullable(infer_type_inner(right, scope, catalog));
            match op {
                BinaryOperator::Eq
                | BinaryOperator::NotEq
                | BinaryOperator::Lt
                | BinaryOperator::LtEq
                | BinaryOperator::Gt
                | BinaryOperator::GtEq
                | BinaryOperator::And
                | BinaryOperator::Or => AvroType::Boolean,
                BinaryOperator::Plus
                | BinaryOperator::Minus
                | BinaryOperator::Multiply
                | BinaryOperator::Divide
                | BinaryOperator::Modulo => widen_numeric(l, r),
                BinaryOperator::StringConcat => AvroType::String,
                _ => AvroType::String,
            }
        }

        Expr::UnaryOp { expr, .. } => infer_type_inner(expr, scope, catalog),

        // ---- CASE / COALESCE / function ----
        Expr::Case { results, .. } => results
            .first()
            .map(|e| infer_type_inner(e, scope, catalog))
            .unwrap_or(AvroType::String),

        Expr::Function(f) => function_return_type(f),

        // Fallbacks —
        _ => AvroType::String,
    }
}

fn function_return_type(f: &sqlparser::ast::Function) -> AvroType {
    let name = f
        .name
        .0
        .last()
        .map(|i| i.value.to_ascii_lowercase())
        .unwrap_or_default();
    match name.as_str() {
        "count" => AvroType::Long,
        "sum" | "avg" | "stddev" | "variance" | "stddev_pop" | "stddev_samp" | "var_pop"
        | "var_samp" => AvroType::Double,
        "min" | "max" | "first_value" | "last_value" | "nth_value" => AvroType::Double,
        "date_add" | "date_sub" | "current_date" | "to_date" => AvroType::Date,
        "current_timestamp" | "now" | "sysdate" | "to_timestamp" | "from_unixtime" => {
            AvroType::TimestampMillis
        }
        "year" | "month" | "day" | "hour" | "minute" | "second" | "quarter" | "dayofmonth"
        | "weekofyear" => AvroType::Int,
        "length" | "character_length" | "octet_length" | "ascii" => AvroType::Int,
        "upper" | "lower" | "concat" | "concat_ws" | "substring" | "substr" | "trim" | "ltrim"
        | "rtrim" | "replace" | "translate" | "regexp_replace" | "regexp_extract" | "lpad"
        | "rpad" | "reverse" | "to_char" | "date_format" | "format_number" | "soundex"
        | "initcap" => AvroType::String,
        "abs" | "ceil" | "ceiling" | "floor" | "round" | "bround" | "exp" | "ln" | "log"
        | "log10" | "log2" | "pow" | "power" | "sqrt" | "cbrt" | "sign" | "sin" | "cos" | "tan"
        | "asin" | "acos" | "atan" | "atan2" | "degrees" | "radians" | "mod" | "pmod" => {
            AvroType::Double
        }
        "md5" | "sha" | "sha1" | "hex" => AvroType::String,
        "crc32" => AvroType::Long,
        "rand" | "random" => AvroType::Double,
        _ => AvroType::String,
    }
}

fn value_to_avro(v: &Value) -> AvroType {
    match v {
        Value::Number(s, _) => {
            if s.contains('.') || s.contains('e') || s.contains('E') {
                AvroType::Double
            } else {
                AvroType::Long
            }
        }
        Value::SingleQuotedString(_)
        | Value::DoubleQuotedString(_)
        | Value::EscapedStringLiteral(_)
        | Value::DollarQuotedString(_)
        | Value::NationalStringLiteral(_)
        | Value::HexStringLiteral(_)
        | Value::SingleQuotedByteStringLiteral(_)
        | Value::DoubleQuotedByteStringLiteral(_) => AvroType::String,
        Value::Boolean(_) => AvroType::Boolean,
        Value::Null => AvroType::String, // caller wraps in Nullable anyway
        _ => AvroType::String,
    }
}

fn data_type_to_avro(dt: &sqlparser::ast::DataType) -> AvroType {
    use sqlparser::ast::DataType as D;
    match dt {
        D::TinyInt(_)
        | D::UnsignedTinyInt(_)
        | D::SmallInt(_)
        | D::UnsignedSmallInt(_)
        | D::Int(_)
        | D::Integer(_)
        | D::UnsignedInt(_)
        | D::UnsignedInteger(_)
        | D::Int4(_)
        | D::Int2(_) => AvroType::Int,
        D::BigInt(_) | D::UnsignedBigInt(_) | D::Int8(_) => AvroType::Long,
        D::Real | D::Float4 => AvroType::Float,
        D::Double | D::DoublePrecision | D::Float8 | D::Float(_) => AvroType::Double,
        D::Boolean | D::Bool => AvroType::Boolean,
        D::Binary(_) | D::Varbinary(_) | D::Blob(_) | D::Bytes(_) => AvroType::Bytes,
        D::Date => AvroType::Date,
        D::Timestamp(_, _) | D::Datetime(_) => AvroType::TimestampMillis,
        D::Decimal(info) | D::Numeric(info) | D::BigNumeric(info) | D::Dec(info) => {
            let (p, s) = match info {
                sqlparser::ast::ExactNumberInfo::Precision(p) => (*p as u32, 0u32),
                sqlparser::ast::ExactNumberInfo::PrecisionAndScale(p, s) => (*p as u32, *s as u32),
                sqlparser::ast::ExactNumberInfo::None => (18, 2),
            };
            AvroType::Decimal {
                precision: p,
                scale: s,
            }
        }
        _ => AvroType::String,
    }
}

fn widen_numeric(l: AvroType, r: AvroType) -> AvroType {
    let rank = |t: &AvroType| match t {
        AvroType::Boolean => 0,
        AvroType::Int => 1,
        AvroType::Long => 2,
        AvroType::Float => 3,
        AvroType::Double => 4,
        AvroType::Decimal { .. } => 4,
        _ => 5, // strings / other: treat as widest
    };
    if rank(&l) >= rank(&r) {
        l
    } else {
        r
    }
}

fn lookup_bare_column<C: TypedCatalog>(
    col: &str,
    scope: &[FromEntry],
    catalog: &C,
) -> Option<AvroType> {
    // Single-table scope: look up directly.
    if scope.len() == 1 {
        let e = &scope[0];
        return catalog
            .column_type(&e.db, &e.table, col)
            .map(|t| hive_type_to_avro(&t));
    }
    // Multi-table scope: first table with matching column wins.
    for e in scope {
        if let Some(t) = catalog.column_type(&e.db, &e.table, col) {
            return Some(hive_type_to_avro(&t));
        }
    }
    None
}

fn lookup_qualified_column<C: TypedCatalog>(
    qualifier: &str,
    col: &str,
    scope: &[FromEntry],
    catalog: &C,
) -> Option<AvroType> {
    for e in scope {
        if e.alias.eq_ignore_ascii_case(qualifier) || e.table.eq_ignore_ascii_case(qualifier) {
            return catalog
                .column_type(&e.db, &e.table, col)
                .map(|t| hive_type_to_avro(&t));
        }
    }
    None
}

// -------------------------------------------------------------------
// Hive type string → Avro
// -------------------------------------------------------------------

fn hive_type_to_avro(t: &str) -> AvroType {
    let t = t.trim();
    let upper = t.to_ascii_uppercase();
    // Primitive names (may carry parens — `VARCHAR(64)` etc — or angle
    // brackets — `ARRAY<INT>`, `MAP<K,V>`, `STRUCT<...>`).
    let stop = upper.find(['(', '<']).unwrap_or(upper.len());
    let head = upper[..stop].trim();
    match head {
        "BOOLEAN" | "BOOL" => AvroType::Boolean,
        "TINYINT" | "SMALLINT" | "INT" | "INT4" | "INT2" | "INTEGER" => AvroType::Int,
        "BIGINT" | "INT8" | "LONG" => AvroType::Long,
        "FLOAT" | "REAL" | "FLOAT4" => AvroType::Float,
        "DOUBLE" | "DOUBLE PRECISION" | "FLOAT8" => AvroType::Double,
        "DECIMAL" | "NUMERIC" | "DEC" => {
            let (p, s) = parse_decimal_params(t).unwrap_or((18, 2));
            AvroType::Decimal {
                precision: p,
                scale: s,
            }
        }
        "STRING" | "VARCHAR" | "CHAR" | "TEXT" | "UUID" | "JSON" | "JSONB" => AvroType::String,
        "BINARY" | "VARBINARY" | "BYTEA" | "BLOB" => AvroType::Bytes,
        "DATE" => AvroType::Date,
        "TIMESTAMP" | "TIMESTAMPTZ" | "TIMESTAMP WITH TIME ZONE" | "DATETIME" => {
            AvroType::TimestampMillis
        }
        "ARRAY" => {
            if let Some(inner) = parse_angle(t) {
                AvroType::Array(Box::new(hive_type_to_avro(&inner)))
            } else {
                AvroType::Array(Box::new(AvroType::String))
            }
        }
        "MAP" => {
            if let Some(inner) = parse_angle(t) {
                // Hive MAP<K, V> — Avro only allows string keys; we use the
                // value type as given.
                let parts = split_top_level_comma(&inner);
                let v = parts
                    .get(1)
                    .cloned()
                    .unwrap_or_else(|| "STRING".to_string());
                AvroType::Map(Box::new(hive_type_to_avro(&v)))
            } else {
                AvroType::Map(Box::new(AvroType::String))
            }
        }
        "STRUCT" => {
            if let Some(inner) = parse_angle(t) {
                let parts = split_top_level_comma(&inner);
                let fields: Vec<AvroField> = parts
                    .iter()
                    .enumerate()
                    .map(|(i, p)| {
                        let mut it = p.splitn(2, ':');
                        let name = it
                            .next()
                            .map(str::trim)
                            .map(str::to_string)
                            .unwrap_or_else(|| format!("f{i}"));
                        let ty = it.next().map(str::trim).unwrap_or("STRING");
                        AvroField {
                            name,
                            avro_type: nullable(hive_type_to_avro(ty)),
                            doc: None,
                        }
                    })
                    .collect();
                AvroType::Record(Box::new(AvroRecord {
                    name: "_struct".to_string(),
                    namespace: None,
                    fields,
                }))
            } else {
                AvroType::String
            }
        }
        _ => AvroType::String,
    }
}

fn parse_decimal_params(t: &str) -> Option<(u32, u32)> {
    let open = t.find('(')?;
    let close = t.rfind(')')?;
    let params = &t[open + 1..close];
    let mut it = params.split(',').map(str::trim);
    let p: u32 = it.next()?.parse().ok()?;
    let s: u32 = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    Some((p, s))
}

fn parse_angle(t: &str) -> Option<String> {
    let open = t.find('<')?;
    let close = t.rfind('>')?;
    if close <= open {
        return None;
    }
    Some(t[open + 1..close].to_string())
}

/// Split on commas that are at angle-bracket depth 0 — so `MAP<STRING,
/// STRUCT<a:INT,b:STRING>>` splits into `["STRING", "STRUCT<a:INT,b:STRING>"]`.
fn split_top_level_comma(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '<' => {
                depth += 1;
                cur.push(c);
            }
            '>' => {
                depth -= 1;
                cur.push(c);
            }
            ',' if depth == 0 => {
                out.push(std::mem::take(&mut cur).trim().to_string());
            }
            _ => cur.push(c),
        }
    }
    if !cur.is_empty() {
        out.push(cur.trim().to_string());
    }
    out
}

// -------------------------------------------------------------------
// Nullability helpers
// -------------------------------------------------------------------

fn nullable(t: AvroType) -> AvroType {
    match t {
        AvroType::Nullable(_) => t,
        _ => AvroType::Nullable(Box::new(t)),
    }
}

fn strip_nullable(t: AvroType) -> AvroType {
    match t {
        AvroType::Nullable(inner) => *inner,
        other => other,
    }
}
