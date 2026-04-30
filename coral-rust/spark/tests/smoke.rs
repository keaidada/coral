// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// End-to-end tests for coral-spark.

use coral_core::InMemoryCatalog;
use coral_spark::{
    analyze_plan, classify_predicate, prepare_view, PredicateClass,
};

// ---------- prepare_view (session-free subset of coral-spark-catalog) ----------

fn employees_catalog() -> InMemoryCatalog {
    InMemoryCatalog::from_pairs(&[
        ("hr", "employees", &["id|BIGINT", "name|VARCHAR", "salary|DOUBLE"]),
        ("hr", "departments", &["id|INT", "name|VARCHAR"]),
    ])
}

#[test]
fn prepare_view_emits_spark_sql_and_avro() {
    let cat = employees_catalog();
    let v = prepare_view(
        "CREATE VIEW vw AS SELECT id, NVL(name, 'n/a') AS nm FROM hr.employees",
        &cat,
    )
    .unwrap();

    assert_eq!(v.name, "vw");
    assert!(v.spark_sql.contains("COALESCE"), "{}", v.spark_sql);
    assert!(!v.spark_sql.contains("NVL("), "{}", v.spark_sql);
    assert!(v.avro_schema.contains("\"type\": \"record\""), "{}", v.avro_schema);
    assert!(v.avro_schema.contains("\"name\": \"vw\""), "{}", v.avro_schema);
}

#[test]
fn prepare_view_collects_referenced_tables() {
    let cat = employees_catalog();
    let v = prepare_view(
        "CREATE VIEW j AS
            SELECT e.id, d.name
            FROM hr.employees e
            JOIN hr.departments d ON e.id = d.id",
        &cat,
    )
    .unwrap();
    assert_eq!(
        v.referenced_tables,
        vec!["hr.departments".to_string(), "hr.employees".to_string()]
    );
}

#[test]
fn prepare_view_accepts_bare_select() {
    let cat = employees_catalog();
    let v = prepare_view("SELECT id FROM hr.employees", &cat).unwrap();
    assert_eq!(v.name, "view");
    assert!(v.referenced_tables.contains(&"hr.employees".to_string()));
}

// ---------- classify_predicate (scalar primitive) ----------

#[test]
fn simple_equality_is_simple() {
    assert_eq!(classify_predicate("id = 1"), PredicateClass::Simple);
    assert_eq!(classify_predicate("x > 10"), PredicateClass::Simple);
    assert_eq!(classify_predicate("x <= 10 AND y > 5"), PredicateClass::Simple);
    assert_eq!(
        classify_predicate("dept IN ('eng', 'sales')"),
        PredicateClass::Simple
    );
    assert_eq!(classify_predicate("x IS NULL"), PredicateClass::Simple);
    assert_eq!(
        classify_predicate("x BETWEEN 1 AND 10"),
        PredicateClass::Simple
    );
}

#[test]
fn function_call_is_complicated() {
    assert_eq!(
        classify_predicate("substring(name, 1, 3) = 'abc'"),
        PredicateClass::Complicated
    );
    assert_eq!(
        classify_predicate("datediff(current_date, hired) > 30"),
        PredicateClass::Complicated
    );
    assert_eq!(
        classify_predicate("upper(name) = 'ALICE'"),
        PredicateClass::Complicated
    );
}

#[test]
fn cast_is_complicated() {
    assert_eq!(
        classify_predicate("CAST(id AS BIGINT) = 10"),
        PredicateClass::Complicated
    );
}

#[test]
fn case_expression_is_complicated() {
    assert_eq!(
        classify_predicate("CASE WHEN x > 0 THEN 1 ELSE 0 END = 1"),
        PredicateClass::Complicated
    );
}

#[test]
fn subquery_is_complicated() {
    assert_eq!(
        classify_predicate("id IN (SELECT id FROM other)"),
        PredicateClass::Complicated
    );
}

#[test]
fn gibberish_is_unparseable() {
    assert_eq!(classify_predicate(""), PredicateClass::Unparseable);
    assert_eq!(
        classify_predicate(";;;garbage"),
        PredicateClass::Unparseable
    );
}

// ---------- analyze_plan (multi-scan text parser) ----------

#[test]
fn analyze_simple_spark_plan() {
    let plan = r"
        == Physical Plan ==
        *(1) Project [id#12, name#13]
        +- *(1) Filter (id#12 > 0)
           +- FileScan parquet hr.employees[id#12,name#13] Batched: true, DataFilters: [isnotnull(id#12)], Format: Parquet, PushedFilters: [IsNotNull(id), GreaterThan(id,0)], ReadSchema: struct<id:bigint,name:string>
    ";
    let results = analyze_plan(plan);
    assert_eq!(results.len(), 1);
    let r = &results[0];
    assert!(r.table.contains("hr.employees"), "{:?}", r.table);
    // Two predicates — both simple.
    assert_eq!(r.predicates.len(), 2);
    for p in &r.predicates {
        assert_eq!(p.class, PredicateClass::Simple, "pred={}", p.predicate);
    }
}

#[test]
fn analyze_plan_flags_complicated_pushdown() {
    // Predicate uses datediff — should be flagged Complicated.
    let plan = r"
        == Physical Plan ==
        FileScan parquet hr.events[id#0,ts#1] Batched: true, PushedFilters: [datediff(current_date(), ts) > 30, id > 0], ReadSchema: struct<id:bigint,ts:timestamp>
    ";
    let results = analyze_plan(plan);
    assert_eq!(results.len(), 1);
    let preds = &results[0].predicates;
    assert_eq!(preds.len(), 2);
    let complicated_count = preds
        .iter()
        .filter(|p| p.class == PredicateClass::Complicated)
        .count();
    let simple_count = preds
        .iter()
        .filter(|p| p.class == PredicateClass::Simple)
        .count();
    assert_eq!(complicated_count, 1, "{preds:?}");
    assert_eq!(simple_count, 1, "{preds:?}");
}

#[test]
fn analyze_plan_handles_multiple_scans() {
    let plan = r"
        *(3) Join Inner, (id#1 = id#10)
        :- FileScan parquet hr.employees[id#1,name#2] PushedFilters: [IsNotNull(id)], ReadSchema: struct<id:bigint,name:string>
        +- FileScan parquet hr.departments[id#10,dept#11] PushedFilters: [IsNotNull(id), id > 0], ReadSchema: struct<id:int,dept:string>
    ";
    let results = analyze_plan(plan);
    assert_eq!(results.len(), 2);
    assert!(results[0].table.contains("hr.employees"));
    assert!(results[1].table.contains("hr.departments"));
    assert_eq!(results[0].predicates.len(), 1);
    assert_eq!(results[1].predicates.len(), 2);
}

#[test]
fn analyze_plan_empty_when_no_scans() {
    let plan = "not a real plan";
    let results = analyze_plan(plan);
    assert!(results.is_empty());
}
