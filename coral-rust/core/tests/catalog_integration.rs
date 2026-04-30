// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Integration tests for the Stage-4 catalog layer.

use coral_core::{translate_with_catalog, InMemoryCatalog, ValidationIssue};

fn sample_catalog() -> InMemoryCatalog {
    InMemoryCatalog::from_pairs(&[
        (
            "default",
            "employees",
            &[
                "id|int",
                "name|string",
                "dept_id|int",
                "salary|double",
                "mgr_id|int",
            ],
        ),
        ("default", "departments", &["id|int", "name|string"]),
    ])
}

#[test]
fn translate_with_catalog_no_issues_when_all_resolves() {
    let cat = sample_catalog();
    let result = translate_with_catalog(
        "SELECT e.id, e.name FROM default.employees e WHERE e.salary > 0",
        &cat,
    )
    .unwrap();
    assert!(result.issues.is_empty(), "issues: {:?}", result.issues);
    assert!(
        result.spark_sql.contains("FROM default.employees"),
        "got: {}",
        result.spark_sql
    );
}

#[test]
fn translate_with_catalog_flags_unknown_column_with_suggestion() {
    let cat = sample_catalog();
    let result = translate_with_catalog("SELECT e.dpt_id FROM default.employees e", &cat).unwrap();
    assert_eq!(result.issues.len(), 1);
    match &result.issues[0] {
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
    // Translation still proceeds — caller can use the SQL verbatim.
    assert!(
        result.spark_sql.contains("SELECT e.dpt_id"),
        "got: {}",
        result.spark_sql
    );
}

#[test]
fn translate_with_catalog_flags_unknown_table() {
    let cat = sample_catalog();
    let result = translate_with_catalog("SELECT * FROM default.nonesuch", &cat).unwrap();
    assert_eq!(
        result.issues,
        vec![ValidationIssue::UnknownTable {
            db: "default".into(),
            table: "nonesuch".into(),
        }]
    );
}

#[test]
fn translate_with_catalog_still_runs_rewrites() {
    // Catalog validation must not bypass the normal GaussDB -> Spark rewrites.
    let cat = sample_catalog();
    let result = translate_with_catalog(
        "SELECT NVL(e.name, 'n/a'), e.salary::BIGINT FROM default.employees e",
        &cat,
    )
    .unwrap();
    assert!(result.issues.is_empty(), "{:?}", result.issues);
    // Function rewrite:
    assert!(
        result.spark_sql.contains("COALESCE(e.name, 'n/a')"),
        "got: {}",
        result.spark_sql
    );
    // Cast syntax rewrite:
    assert!(
        result.spark_sql.contains("CAST(e.salary AS BIGINT)"),
        "got: {}",
        result.spark_sql
    );
}

#[test]
fn translate_with_catalog_handles_cte_silently() {
    // The validator's unqualified-reference path intentionally doesn't
    // resolve against CTEs — confirm that doesn't produce false positives.
    let cat = sample_catalog();
    let sql = "WITH active AS (SELECT id FROM default.employees)
               SELECT id FROM active";
    let result = translate_with_catalog(sql, &cat).unwrap();
    assert!(result.issues.is_empty(), "{:?}", result.issues);
}

#[test]
fn translate_with_catalog_respects_aliases() {
    let cat = sample_catalog();
    // Alias `emp` must resolve to `employees`.
    let result =
        translate_with_catalog("SELECT emp.salary FROM default.employees emp", &cat).unwrap();
    assert!(result.issues.is_empty(), "{:?}", result.issues);
}
