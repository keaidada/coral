// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// End-to-end tests for coral-schema's view → Avro schema inference.
//
// The fixtures mirror the style of Java coral-schema's ViewToAvroSchemaConverter
// test cases: pair a SQL DDL with an InMemoryCatalog seeded with the referenced
// tables, run the converter, and assert the emitted JSON matches the expected
// Avro shape.

use coral_core::InMemoryCatalog;
use coral_schema::{to_avro_record, to_avro_schema, AvroField, AvroType};

fn employees_catalog() -> InMemoryCatalog {
    InMemoryCatalog::from_pairs(&[
        (
            "hr",
            "employees",
            &[
                "id|BIGINT",
                "name|VARCHAR",
                "email|VARCHAR",
                "salary|DOUBLE",
                "dept_id|INT",
                "active|BOOLEAN",
                "hired|DATE",
                "updated|TIMESTAMP",
                "notes|STRING",
                "tags|ARRAY<STRING>",
                "attrs|MAP<STRING,STRING>",
                "address|STRUCT<street:STRING,city:STRING,zip:INT>",
                "scale|DECIMAL(10,4)",
                "blob|BYTEA",
            ],
        ),
        (
            "hr",
            "departments",
            &[
                "id|INT",
                "name|VARCHAR",
                "floor|INT",
            ],
        ),
    ])
}

fn find_field<'a>(rec: &'a coral_schema::AvroRecord, name: &str) -> &'a AvroField {
    rec.fields
        .iter()
        .find(|f| f.name.eq_ignore_ascii_case(name))
        .unwrap_or_else(|| panic!("no field {name} in {:?}", rec.fields))
}

fn unwrap_nullable(t: &AvroType) -> &AvroType {
    match t {
        AvroType::Nullable(inner) => inner,
        other => other,
    }
}

// ---------- simple projections ----------

#[test]
fn plain_select_preserves_column_types() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT id, name, salary, active FROM hr.employees",
        &cat,
    )
    .unwrap();

    assert_eq!(rec.name, "v");
    assert_eq!(rec.fields.len(), 4);
    assert_eq!(unwrap_nullable(&find_field(&rec, "id").avro_type), &AvroType::Long);
    assert_eq!(unwrap_nullable(&find_field(&rec, "name").avro_type), &AvroType::String);
    assert_eq!(unwrap_nullable(&find_field(&rec, "salary").avro_type), &AvroType::Double);
    assert_eq!(unwrap_nullable(&find_field(&rec, "active").avro_type), &AvroType::Boolean);
}

#[test]
fn aliases_drive_field_names() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT id AS emp_id, name AS full_name FROM hr.employees",
        &cat,
    )
    .unwrap();
    assert_eq!(rec.fields[0].name, "emp_id");
    assert_eq!(rec.fields[1].name, "full_name");
}

#[test]
fn date_and_timestamp_get_logical_types() {
    let cat = employees_catalog();
    let json = to_avro_schema(
        "CREATE VIEW v AS SELECT hired, updated FROM hr.employees",
        &cat,
    )
    .unwrap();
    assert!(json.contains("\"logicalType\": \"date\""), "{json}");
    assert!(json.contains("\"logicalType\": \"timestamp-millis\""), "{json}");
}

#[test]
fn decimal_carries_precision_and_scale() {
    let cat = employees_catalog();
    let json = to_avro_schema(
        "CREATE VIEW v AS SELECT scale FROM hr.employees",
        &cat,
    )
    .unwrap();
    assert!(json.contains("\"logicalType\": \"decimal\""), "{json}");
    assert!(json.contains("\"precision\": 10"), "{json}");
    assert!(json.contains("\"scale\": 4"), "{json}");
}

// ---------- complex types ----------

#[test]
fn array_column_becomes_avro_array() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT tags FROM hr.employees",
        &cat,
    )
    .unwrap();
    let f = find_field(&rec, "tags");
    match unwrap_nullable(&f.avro_type) {
        AvroType::Array(item) => assert_eq!(**item, AvroType::String),
        other => panic!("expected Array, got {other:?}"),
    }
}

#[test]
fn map_column_becomes_avro_map() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT attrs FROM hr.employees",
        &cat,
    )
    .unwrap();
    let f = find_field(&rec, "attrs");
    match unwrap_nullable(&f.avro_type) {
        AvroType::Map(val) => assert_eq!(**val, AvroType::String),
        other => panic!("expected Map, got {other:?}"),
    }
}

#[test]
fn struct_column_becomes_avro_record() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT address FROM hr.employees",
        &cat,
    )
    .unwrap();
    let f = find_field(&rec, "address");
    match unwrap_nullable(&f.avro_type) {
        AvroType::Record(r) => {
            let names: Vec<_> = r.fields.iter().map(|f| f.name.clone()).collect();
            assert_eq!(names, vec!["street", "city", "zip"]);
        }
        other => panic!("expected Record, got {other:?}"),
    }
}

#[test]
fn bytea_column_becomes_bytes() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT blob FROM hr.employees",
        &cat,
    )
    .unwrap();
    let f = find_field(&rec, "blob");
    assert_eq!(unwrap_nullable(&f.avro_type), &AvroType::Bytes);
}

// ---------- expressions ----------

#[test]
fn arithmetic_widens_to_widest_operand() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT salary + dept_id AS total FROM hr.employees",
        &cat,
    )
    .unwrap();
    let f = find_field(&rec, "total");
    assert_eq!(unwrap_nullable(&f.avro_type), &AvroType::Double);
}

#[test]
fn count_is_long() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT COUNT(*) AS n FROM hr.employees",
        &cat,
    )
    .unwrap();
    assert_eq!(unwrap_nullable(&find_field(&rec, "n").avro_type), &AvroType::Long);
}

#[test]
fn length_is_int() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT LENGTH(name) AS name_len FROM hr.employees",
        &cat,
    )
    .unwrap();
    assert_eq!(unwrap_nullable(&find_field(&rec, "name_len").avro_type), &AvroType::Int);
}

#[test]
fn string_concat_yields_string() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT name || '-' || email AS handle FROM hr.employees",
        &cat,
    )
    .unwrap();
    assert_eq!(unwrap_nullable(&find_field(&rec, "handle").avro_type), &AvroType::String);
}

#[test]
fn cast_as_bigint_yields_long() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT CAST(dept_id AS BIGINT) AS big_dept FROM hr.employees",
        &cat,
    )
    .unwrap();
    assert_eq!(
        unwrap_nullable(&find_field(&rec, "big_dept").avro_type),
        &AvroType::Long
    );
}

#[test]
fn bare_select_without_create_view_uses_default_name() {
    let cat = employees_catalog();
    let rec = to_avro_record("SELECT id FROM hr.employees", &cat).unwrap();
    assert_eq!(rec.name, "view");
    assert_eq!(rec.fields.len(), 1);
}

// ---------- joins + qualified refs ----------

#[test]
fn qualified_column_ref_resolves_via_alias() {
    let cat = employees_catalog();
    let rec = to_avro_record(
        "CREATE VIEW v AS SELECT e.id, d.name AS dept_name
            FROM hr.employees e JOIN hr.departments d ON e.dept_id = d.id",
        &cat,
    )
    .unwrap();
    assert_eq!(unwrap_nullable(&find_field(&rec, "id").avro_type), &AvroType::Long);
    assert_eq!(unwrap_nullable(&find_field(&rec, "dept_name").avro_type), &AvroType::String);
}

// ---------- JSON shape sanity ----------

#[test]
fn json_shape_is_valid_avro() {
    let cat = employees_catalog();
    let json = to_avro_schema(
        "CREATE VIEW v AS SELECT id, name, salary, tags FROM hr.employees",
        &cat,
    )
    .unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "record");
    assert_eq!(v["name"], "v");
    let fields = v["fields"].as_array().unwrap();
    assert_eq!(fields.len(), 4);
    let id_field = &fields[0];
    assert_eq!(id_field["name"], "id");
    // Nullable → ["null", <inner>]
    let t = id_field["type"].as_array().unwrap();
    assert_eq!(t[0], "null");
    assert_eq!(t[1], "long");
}
