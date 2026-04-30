// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.
//
// End-to-end tests for the HTTP surface.
//
// Uses axum's `oneshot` on a Router + a Body-in-Body-out helper so no
// real TCP port / tokio::spawn is needed. Each test is synchronous from
// the outside (wrapped in #[tokio::test]), no background task or
// runtime teardown.

use axum::{
    body::Body,
    http::{header, Method, Request, StatusCode},
    Router,
};
use serde_json::json;
use tower::util::ServiceExt; // for `oneshot`

use coral_service::router;

async fn post_json(path: &str, body: serde_json::Value) -> (StatusCode, serde_json::Value) {
    post_json_on(&router(), path, body).await
}

async fn post_json_on(
    app: &Router,
    path: &str,
    body: serde_json::Value,
) -> (StatusCode, serde_json::Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap_or(serde_json::Value::Null);
    (status, v)
}

async fn get(path: &str) -> (StatusCode, axum::body::Bytes, Option<String>) {
    get_on(&router(), path).await
}

async fn get_on(
    app: &Router,
    path: &str,
) -> (StatusCode, axum::body::Bytes, Option<String>) {
    let req = Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let ct = resp
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);
    let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    (status, bytes, ct)
}

#[tokio::test]
async fn health_returns_ok() {
    let (status, body) = post_json("/api/health", json!({})).await;
    // POST to a GET-only endpoint should 405, so use GET.
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);

    let (s, bytes, _) = get("/api/health").await;
    assert_eq!(s, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["status"], "ok");
    assert!(v["version"].is_string());
    let _ = body;
}

#[tokio::test]
async fn translate_to_spark_is_default() {
    let (status, v) = post_json(
        "/api/translations/translate",
        json!({ "query": "SELECT NVL(a, 0) FROM t" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["target"], "spark");
    let translated = v["translated"].as_str().unwrap();
    assert!(translated.to_uppercase().contains("COALESCE"), "{translated}");
    assert!(v["error"].is_null());
}

#[tokio::test]
async fn translate_to_trino_explicit() {
    let (status, v) = post_json(
        "/api/translations/translate",
        json!({
            "query": "SELECT GET_JSON_OBJECT(p, '$.k'), RAND() FROM t",
            "targetLanguage": "trino",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["target"], "trino");
    let t = v["translated"].as_str().unwrap().to_uppercase();
    assert!(t.contains("JSON_EXTRACT"), "{t}");
    assert!(t.contains("RANDOM()"), "{t}");
}

#[tokio::test]
async fn translate_returns_error_field_on_parse_failure() {
    let (status, v) = post_json(
        "/api/translations/translate",
        json!({ "query": "SELEKT garbage" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK); // 200 with error field, matches Java service behavior
    assert_eq!(v["translated"], "");
    assert!(v["error"].is_string());
}

#[tokio::test]
async fn validate_recognizes_good_sql() {
    let (status, v) = post_json(
        "/api/translations/validate",
        json!({ "query": "SELECT 1; SELECT 2" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["parses"], true);
    assert_eq!(v["statementCount"], 2);
}

#[tokio::test]
async fn validate_reports_parse_error() {
    let (status, v) = post_json(
        "/api/translations/validate",
        json!({ "query": "SELEKT" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["parses"], false);
    assert!(v["parseError"].is_string());
}

#[tokio::test]
async fn list_functions_returns_full_registry() {
    let (status, bytes, _) = get("/api/functions").await;
    assert_eq!(status, StatusCode::OK);
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let total = v["total"].as_u64().unwrap();
    assert!(total >= 100, "expected >=100 function entries, got {total}");
    let entries = v["entries"].as_array().unwrap();
    assert_eq!(entries.len() as u64, total);
    // Spot-check that a known rename is represented.
    let nvl = entries
        .iter()
        .find(|e| e["name"] == "nvl")
        .expect("nvl should be present");
    assert!(nvl["disposition"].as_str().unwrap().starts_with("rename"));
}

#[tokio::test]
async fn visualize_generate_and_fetch_roundtrip() {
    let app = router();
    let (_, v) = post_json_on(
        &app,
        "/api/visualizations/generategraphs",
        json!({ "query": "SELECT 1; SELECT 2", "format": "dot" }),
    )
    .await;
    let id = v["graphId"].as_str().expect("graphId").to_string();
    assert_eq!(v["format"], "dot");

    let (s, bytes, ct) = get_on(&app, &format!("/api/visualizations/{id}")).await;
    assert_eq!(s, StatusCode::OK);
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(body.starts_with("digraph"), "{body}");
    assert!(body.contains("stmt0"));
    assert!(body.contains("stmt1"));
    assert!(body.contains("stmt0 -> stmt1"));
    let ct = ct.unwrap_or_default();
    assert!(ct.contains("graphviz") || ct.contains("text"), "{ct}");
}

#[tokio::test]
async fn visualize_plantuml_format() {
    let app = router();
    let (_, v) = post_json_on(
        &app,
        "/api/visualizations/generategraphs",
        json!({ "query": "SELECT 1", "format": "plantuml" }),
    )
    .await;
    let id = v["graphId"].as_str().unwrap().to_string();
    let (_, bytes, _) = get_on(&app, &format!("/api/visualizations/{id}")).await;
    let body = std::str::from_utf8(&bytes).unwrap();
    assert!(body.contains("@startuml"), "{body}");
    assert!(body.contains("@enduml"), "{body}");
}

#[tokio::test]
async fn visualize_unknown_format_rejected() {
    let (status, v) = post_json(
        "/api/visualizations/generategraphs",
        json!({ "query": "SELECT 1", "format": "mermaid" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(v["error"].is_string());
}

#[tokio::test]
async fn visualize_missing_id_returns_404() {
    let (s, _, _) = get("/api/visualizations/ffffffffffffffff").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}
