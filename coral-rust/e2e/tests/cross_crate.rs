// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// Cross-crate end-to-end test that proves every backend in the
// coral-rust workspace can consume the same input and produce coherent
// output. Uses a realistic GaussDB-flavored query and verifies each
// backend's contribution is reasonable.

use coral_core::{InMemoryCatalog, Target};

const SAMPLE_SQL: &str = r#"
    SELECT
        e.id,
        NVL(d.name, 'Unknown')      AS dept,
        COUNT(*)                    AS headcount,
        AVG(e.salary)               AS avg_salary
    FROM hr.employees e
    LEFT JOIN hr.departments d ON e.dept_id = d.id
    WHERE e.salary > 0
    GROUP BY e.id, d.name
"#;

fn sample_catalog() -> InMemoryCatalog {
    InMemoryCatalog::from_pairs(&[
        (
            "hr",
            "employees",
            &[
                "id|BIGINT",
                "name|VARCHAR",
                "salary|DOUBLE",
                "dept_id|INT",
            ],
        ),
        ("hr", "departments", &["id|INT", "name|VARCHAR"]),
    ])
}

// ---------- core translator ----------

#[test]
fn core_translates_to_spark_sql() {
    let spark = coral_core::translate(SAMPLE_SQL).unwrap();
    // NVL -> COALESCE.
    assert!(spark.to_uppercase().contains("COALESCE"), "{spark}");
    assert!(!spark.to_uppercase().contains("NVL("), "{spark}");
    assert!(spark.to_uppercase().contains("LEFT JOIN"), "{spark}");
}

#[test]
fn core_translates_to_trino_sql() {
    let trino = coral_core::translate_to(SAMPLE_SQL, Target::Trino).unwrap();
    // COALESCE stays (Trino-compatible).
    assert!(trino.to_uppercase().contains("COALESCE"), "{trino}");
}

// ---------- Trino facade ----------

#[test]
fn trino_facade_and_core_agree() {
    let a = coral_core::translate_to(SAMPLE_SQL, Target::Trino).unwrap();
    let b = coral_trino::to_trino_sql(SAMPLE_SQL).unwrap();
    assert_eq!(a, b);
}

// ---------- Schema inference ----------

#[test]
fn schema_derives_valid_avro_for_view() {
    let cat = sample_catalog();
    let ddl = format!("CREATE VIEW stats AS {SAMPLE_SQL}");
    let json = coral_schema::to_avro_schema(&ddl, &cat).unwrap();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(v["type"], "record");
    assert_eq!(v["name"], "stats");
    let fields = v["fields"].as_array().unwrap();
    let names: Vec<_> = fields.iter().map(|f| f["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["id", "dept", "headcount", "avg_salary"]);
    // COUNT(*) → long (nullable union).
    let headcount_type = &fields[2]["type"];
    let arr = headcount_type.as_array().unwrap();
    assert_eq!(arr[0], "null");
    assert_eq!(arr[1], "long");
}

// ---------- Spark view prep ----------

#[test]
fn spark_prepare_view_matches_referenced_tables() {
    let cat = sample_catalog();
    let ddl = format!("CREATE VIEW stats AS {SAMPLE_SQL}");
    let v = coral_spark::prepare_view(&ddl, &cat).unwrap();
    assert_eq!(v.name, "stats");
    assert!(v.spark_sql.to_uppercase().contains("COALESCE"));
    assert_eq!(
        v.referenced_tables,
        vec!["hr.departments".to_string(), "hr.employees".to_string()]
    );
}

// ---------- Pig Latin ----------

#[test]
fn pig_translator_runs_on_the_sample() {
    let pig = coral_pig::to_pig_latin(SAMPLE_SQL).unwrap();
    assert!(pig.contains("LOAD 'hr.employees'"), "{pig}");
    assert!(pig.contains("LOAD 'hr.departments'"), "{pig}");
    assert!(pig.contains("JOIN LEFT OUTER"), "{pig}");
    assert!(pig.contains("GROUP"), "{pig}");
    assert!(pig.contains("COUNT"), "{pig}");
    assert!(pig.trim_end().ends_with("OUT = t5;") || pig.contains("OUT ="), "{pig}");
}

// ---------- Incremental ----------

#[test]
fn incremental_expands_two_tables_to_three_branches() {
    let expanded = coral_incremental::incremental_sql(SAMPLE_SQL).unwrap();
    // 2 base tables → 3 branches.
    let branch_count = expanded.matches("UNION ALL").count() + 1;
    assert_eq!(branch_count, 3, "{expanded}");
    assert!(expanded.contains("employees_delta"), "{expanded}");
    assert!(expanded.contains("departments_delta"), "{expanded}");
}

// ---------- Visualization ----------

#[test]
fn viz_dot_contains_expected_nodes() {
    let dot = coral_viz::render(SAMPLE_SQL, coral_viz::Format::Dot).unwrap();
    assert!(dot.contains("Query"), "{dot}");
    assert!(dot.contains("FROM"), "{dot}");
    assert!(dot.contains("WHERE"), "{dot}");
    assert!(dot.contains("GROUP BY"), "{dot}");
    assert!(dot.contains("hr.employees"), "{dot}");
}

// ---------- HTTP service end-to-end ----------

#[tokio::test]
async fn service_translate_endpoint_spark_and_trino() {
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use serde_json::json;
    use tower::util::ServiceExt;

    let app = coral_service::router();
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/translations/translate")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "query": SAMPLE_SQL, "targetLanguage": "spark" }).to_string(),
        ))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["target"], "spark");
    assert!(v["translated"].as_str().unwrap().to_uppercase().contains("COALESCE"));

    // Same query, Trino.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/translations/translate")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "query": SAMPLE_SQL, "targetLanguage": "trino" }).to_string(),
        ))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["target"], "trino");
}

// ---------- Sanity: every backend composes without panics on SmokeDemo ----------

#[test]
fn every_backend_survives_the_six_smoke_samples() {
    // Same 6 samples as the CLI's --smoke demo. Each should return a
    // Result (Ok or Err) — never panic.
    let samples = [
        "WITH a AS (SELECT id FROM t) SELECT * FROM a",
        "SELECT NVL(name, 'x') FROM t",
        "SELECT DECODE(k, 1, 'one', 'other') FROM t",
        "SELECT * FROM a JOIN b ON a.id = b.id",
        "SELECT id::BIGINT FROM t",
        "SELECT COUNT(*) FROM t GROUP BY dept",
    ];

    for s in samples {
        // Spark
        let _ = coral_core::translate(s);
        // Trino
        let _ = coral_trino::to_trino_sql(s);
        // Pig (best effort; not all samples match flat-query shape)
        let _ = coral_pig::to_pig_latin(s);
        // Viz
        let _ = coral_viz::render(s, coral_viz::Format::Dot);
        // Incremental (only runs on samples with base tables)
        let _ = coral_incremental::incremental_sql(s);
    }
}
