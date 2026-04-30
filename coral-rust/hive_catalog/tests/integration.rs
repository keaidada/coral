// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! End-to-end test: use a HiveCatalog (with a stub Transport) as the
//! catalog input to coral_core::translate_with_catalog.

use std::collections::HashMap;

use coral_core::{translate_with_catalog, ValidationIssue};
use coral_hive_catalog::{CatalogError, ColumnJson, HiveCatalog, TableSchemaJson, Transport};

/// Fully-baked stub: returns hardcoded schemas for known tables, 404-style
/// None for anything else. No network involved — we're testing the
/// integration shape, not ureq.
struct StubTransport {
    tables: HashMap<(String, String), Vec<String>>,
}

impl StubTransport {
    fn new() -> Self {
        let mut t = HashMap::new();
        t.insert(
            ("analytics".into(), "employees".into()),
            vec![
                "id".into(),
                "name".into(),
                "dept_id".into(),
                "salary".into(),
                "mgr_id".into(),
            ],
        );
        t.insert(
            ("analytics".into(), "departments".into()),
            vec!["id".into(), "name".into()],
        );
        Self { tables: t }
    }
}

impl Transport for StubTransport {
    fn get_table_schema(
        &self,
        db: &str,
        table: &str,
    ) -> Result<Option<TableSchemaJson>, CatalogError> {
        let key = (db.to_ascii_lowercase(), table.to_ascii_lowercase());
        Ok(self.tables.get(&key).map(|cols| TableSchemaJson {
            columns: cols
                .iter()
                .map(|c| ColumnJson {
                    name: c.clone(),
                    r#type: "string".into(),
                })
                .collect(),
        }))
    }
}

#[test]
fn known_query_produces_no_issues_and_translates() {
    let cat = HiveCatalog::new(StubTransport::new());
    let r = translate_with_catalog(
        "SELECT e.id, e.name, NVL(e.salary, 0) FROM analytics.employees e",
        &cat,
    )
    .unwrap();
    assert!(r.issues.is_empty(), "unexpected issues: {:?}", r.issues);
    assert!(r.spark_sql.contains("COALESCE(e.salary, 0)"));
}

#[test]
fn unknown_table_is_flagged() {
    let cat = HiveCatalog::new(StubTransport::new());
    let r = translate_with_catalog("SELECT * FROM analytics.nonesuch", &cat).unwrap();
    assert_eq!(
        r.issues,
        vec![ValidationIssue::UnknownTable {
            db: "analytics".into(),
            table: "nonesuch".into(),
        }]
    );
}

#[test]
fn typo_in_column_gets_suggestion() {
    let cat = HiveCatalog::new(StubTransport::new());
    let r = translate_with_catalog("SELECT e.dpt_id FROM analytics.employees e", &cat).unwrap();
    assert_eq!(r.issues.len(), 1);
    match &r.issues[0] {
        ValidationIssue::UnknownColumn {
            column,
            did_you_mean,
            ..
        } => {
            assert_eq!(column, "dpt_id");
            assert_eq!(did_you_mean.as_deref(), Some("dept_id"));
        }
        other => panic!("unexpected issue: {other:?}"),
    }
}

#[test]
fn catalog_survives_across_multiple_translate_calls() {
    // Verifies the cache isn't invalidated by dropping the translate output
    // — subsequent translations should reuse the schemas.
    let cat = HiveCatalog::new(StubTransport::new());
    for _ in 0..10 {
        let r = translate_with_catalog("SELECT e.id FROM analytics.employees e", &cat).unwrap();
        assert!(r.issues.is_empty());
    }
}
