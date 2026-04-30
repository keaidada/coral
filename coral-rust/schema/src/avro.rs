// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Avro type / record / field representations used by the inference
//! pass. Deliberately minimal — covers what
//! `ViewToAvroSchemaConverter` in the Java tree emits for flat SELECTs
//! without pulling in the full Apache Avro Rust library (too many
//! deps for a schema-only use case).

use serde::Serialize;

/// An Avro primitive or complex type. `Nullable(inner)` serializes as
/// an Avro union `["null", inner]` which is how optional fields are
/// typically represented.
#[derive(Debug, Clone, PartialEq)]
pub enum AvroType {
    // Primitives
    Boolean,
    Int,
    Long,
    Float,
    Double,
    Bytes,
    String,

    // Logical types on top of primitives.
    Date,            // int + logicalType "date"
    TimestampMillis, // long + logicalType "timestamp-millis"
    Decimal { precision: u32, scale: u32 },

    // Complex
    Array(Box<AvroType>),
    Map(Box<AvroType>),
    Record(Box<AvroRecord>),

    // Union with null (optional field).
    Nullable(Box<AvroType>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct AvroRecord {
    pub name: String,
    pub namespace: Option<String>,
    pub fields: Vec<AvroField>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AvroField {
    pub name: String,
    pub avro_type: AvroType,
    pub doc: Option<String>,
}

// ---- JSON serialization ----
//
// Manual serde implementation — the shape differs enough from a direct
// #[derive(Serialize)] that hand-rolling each variant is simpler than
// fighting serde attributes.

impl Serialize for AvroType {
    fn serialize<S>(&self, ser: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        use serde::ser::SerializeSeq;

        match self {
            AvroType::Boolean => ser.serialize_str("boolean"),
            AvroType::Int => ser.serialize_str("int"),
            AvroType::Long => ser.serialize_str("long"),
            AvroType::Float => ser.serialize_str("float"),
            AvroType::Double => ser.serialize_str("double"),
            AvroType::Bytes => ser.serialize_str("bytes"),
            AvroType::String => ser.serialize_str("string"),

            AvroType::Date => {
                let mut m = ser.serialize_map(Some(2))?;
                m.serialize_entry("type", "int")?;
                m.serialize_entry("logicalType", "date")?;
                m.end()
            }
            AvroType::TimestampMillis => {
                let mut m = ser.serialize_map(Some(2))?;
                m.serialize_entry("type", "long")?;
                m.serialize_entry("logicalType", "timestamp-millis")?;
                m.end()
            }
            AvroType::Decimal { precision, scale } => {
                let mut m = ser.serialize_map(Some(4))?;
                m.serialize_entry("type", "bytes")?;
                m.serialize_entry("logicalType", "decimal")?;
                m.serialize_entry("precision", precision)?;
                m.serialize_entry("scale", scale)?;
                m.end()
            }

            AvroType::Array(items) => {
                let mut m = ser.serialize_map(Some(2))?;
                m.serialize_entry("type", "array")?;
                m.serialize_entry("items", items)?;
                m.end()
            }
            AvroType::Map(values) => {
                let mut m = ser.serialize_map(Some(2))?;
                m.serialize_entry("type", "map")?;
                m.serialize_entry("values", values)?;
                m.end()
            }

            AvroType::Record(r) => r.serialize(ser),

            AvroType::Nullable(inner) => {
                let mut seq = ser.serialize_seq(Some(2))?;
                seq.serialize_element("null")?;
                seq.serialize_element(inner.as_ref())?;
                seq.end()
            }
        }
    }
}

impl Serialize for AvroRecord {
    fn serialize<S>(&self, ser: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let len = 3 + self.namespace.is_some() as usize;
        let mut m = ser.serialize_map(Some(len))?;
        m.serialize_entry("type", "record")?;
        m.serialize_entry("name", &self.name)?;
        if let Some(ns) = &self.namespace {
            m.serialize_entry("namespace", ns)?;
        }
        m.serialize_entry("fields", &self.fields)?;
        m.end()
    }
}

impl Serialize for AvroField {
    fn serialize<S>(&self, ser: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let len = 2 + self.doc.is_some() as usize;
        let mut m = ser.serialize_map(Some(len))?;
        m.serialize_entry("name", &self.name)?;
        m.serialize_entry("type", &self.avro_type)?;
        if let Some(d) = &self.doc {
            m.serialize_entry("doc", d)?;
        }
        m.end()
    }
}
