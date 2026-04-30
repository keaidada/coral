// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Axum handlers implementing the REST surface.
//!
//! Each handler is a plain `async fn` returning `impl IntoResponse` so
//! that error paths can be localized (no anyhow bubbling into the wire
//! format). Errors always materialize as a 200-with-error-field in the
//! body — the Java tree does the same, which lets browser clients treat
//! translation failures as data rather than HTTP exceptions.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json,
};

use crate::{
    models::*,
    state::{AppState, GraphPayload},
};

// ---------------------------------------------------------------------
// GET /api/health
// ---------------------------------------------------------------------

pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

// ---------------------------------------------------------------------
// POST /api/translations/translate
// ---------------------------------------------------------------------

pub async fn translate(Json(req): Json<TranslateRequest>) -> Json<TranslateResponse> {
    let target = req
        .target_language
        .as_deref()
        .and_then(coral_core::Target::parse)
        .unwrap_or(coral_core::Target::Spark);

    match coral_core::translate_to(&req.query, target) {
        Ok(sql) => Json(TranslateResponse {
            translated: sql,
            target: target.to_string(),
            issues: vec![],
            error: None,
        }),
        Err(e) => Json(TranslateResponse {
            translated: String::new(),
            target: target.to_string(),
            issues: vec![],
            error: Some(e.to_string()),
        }),
    }
}

// ---------------------------------------------------------------------
// POST /api/translations/validate
// ---------------------------------------------------------------------

pub async fn validate(Json(req): Json<ValidateRequest>) -> Json<ValidateResponse> {
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;
    match Parser::parse_sql(&PostgreSqlDialect {}, &req.query) {
        Ok(stmts) => Json(ValidateResponse {
            parses: true,
            statement_count: stmts.len(),
            parse_error: None,
        }),
        Err(e) => Json(ValidateResponse {
            parses: false,
            statement_count: 0,
            parse_error: Some(e.to_string()),
        }),
    }
}

// ---------------------------------------------------------------------
// POST /api/visualizations/generategraphs
// ---------------------------------------------------------------------

pub async fn generate_graphs(
    State(state): State<AppState>,
    Json(req): Json<VisualizeRequest>,
) -> impl IntoResponse {
    let format = req
        .format
        .as_deref()
        .unwrap_or("dot")
        .to_ascii_lowercase();
    let source = match format.as_str() {
        "dot" => render_dot(&req.query),
        "plantuml" => render_plantuml(&req.query),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({
                    "error": format!("unknown format {format:?}; expected 'dot' or 'plantuml'"),
                })),
            )
                .into_response();
        }
    };
    // No real UUID crate yet — use an ns-timestamp + counter fallback so
    // we don't pull in `uuid` just for this. Collision odds are fine for
    // an in-memory visualization cache.
    let id = short_id();
    state.store(
        id.clone(),
        GraphPayload {
            format: format.clone(),
            source,
        },
    );
    Json(VisualizeResponse {
        graph_id: id,
        format,
    })
    .into_response()
}

// ---------------------------------------------------------------------
// GET /api/visualizations/:id
// ---------------------------------------------------------------------

pub async fn get_visualization(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.get(&id) {
        Some(p) => {
            // Return as text/plain — the client picks its own rendering
            // tool (graphviz, plantuml.jar, or an online service).
            let mime = if p.format == "plantuml" {
                "text/plain; charset=utf-8"
            } else {
                "text/vnd.graphviz; charset=utf-8"
            };
            (
                StatusCode::OK,
                [(axum::http::header::CONTENT_TYPE, mime)],
                p.source,
            )
                .into_response()
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": format!("no graph with id {id}"),
            })),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------
// GET /api/functions
// ---------------------------------------------------------------------

pub async fn list_functions() -> Json<FunctionListResponse> {
    use coral_core::Disposition;
    let reg = coral_core::function_catalog::registry();
    let mut entries: Vec<FunctionEntryDto> = reg
        .values()
        .map(|e| FunctionEntryDto {
            name: e.lower_name.to_string(),
            disposition: match e.disposition {
                Disposition::Passthrough => "passthrough".to_string(),
                Disposition::Rename(n) => format!("rename:{n}"),
                Disposition::CustomRewrite => "custom".to_string(),
                Disposition::UnsupportedBySpark => "unsupported".to_string(),
            },
            category: format!("{:?}", e.category).to_ascii_lowercase(),
            notes: e.notes.to_string(),
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Json(FunctionListResponse {
        total: entries.len(),
        entries,
    })
}

// ---------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------

fn short_id() -> String {
    // 16-hex-digit id based on ns timestamp + thread-local counter. No
    // cryptographic guarantees, just "unique-enough for an in-memory
    // visualization cache".
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0)
        ^ SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{n:016x}")
}

fn render_dot(sql: &str) -> String {
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;
    let stmts = match Parser::parse_sql(&PostgreSqlDialect {}, sql) {
        Ok(s) => s,
        Err(e) => {
            return format!(
                "digraph coral_parse_error {{\n  node [shape=box]; err [label=\"{}\"];\n}}\n",
                escape_dot(&e.to_string())
            );
        }
    };
    let mut out = String::from("digraph coral_ast {\n  rankdir=LR;\n");
    out.push_str("  node [shape=box, fontname=\"Helvetica\"];\n");
    for (i, stmt) in stmts.iter().enumerate() {
        let label = summarize_statement(stmt);
        out.push_str(&format!(
            "  stmt{i} [label=\"{}\"];\n",
            escape_dot(&label)
        ));
    }
    for i in 1..stmts.len() {
        out.push_str(&format!("  stmt{} -> stmt{};\n", i - 1, i));
    }
    out.push_str("}\n");
    out
}

fn render_plantuml(sql: &str) -> String {
    use sqlparser::dialect::PostgreSqlDialect;
    use sqlparser::parser::Parser;
    let stmts = match Parser::parse_sql(&PostgreSqlDialect {}, sql) {
        Ok(s) => s,
        Err(e) => {
            return format!(
                "@startuml\nnote \"coral parse error: {}\"\n@enduml\n",
                escape_plant(&e.to_string())
            );
        }
    };
    let mut out = String::from("@startuml\n");
    for (i, stmt) in stmts.iter().enumerate() {
        let label = summarize_statement(stmt);
        out.push_str(&format!(
            "rectangle \"#{i}: {}\" as stmt{i}\n",
            escape_plant(&label)
        ));
    }
    for i in 1..stmts.len() {
        out.push_str(&format!("stmt{} --> stmt{}\n", i - 1, i));
    }
    out.push_str("@enduml\n");
    out
}

fn summarize_statement(stmt: &sqlparser::ast::Statement) -> String {
    // A one-line label for the AST node. Good enough for a graph; full
    // tree rendering lives in Stage L (coral-viz).
    use sqlparser::ast::Statement;
    match stmt {
        Statement::Query(_) => "SELECT / query".to_string(),
        Statement::Insert(_) => "INSERT".to_string(),
        Statement::Update { .. } => "UPDATE".to_string(),
        Statement::Delete(_) => "DELETE".to_string(),
        Statement::CreateTable(_) => "CREATE TABLE".to_string(),
        Statement::CreateView { .. } => "CREATE VIEW".to_string(),
        Statement::Drop { .. } => "DROP".to_string(),
        Statement::AlterTable { .. } => "ALTER TABLE".to_string(),
        Statement::Merge { .. } => "MERGE".to_string(),
        _ => format!("{stmt}")
            .chars()
            .take(60)
            .collect::<String>(),
    }
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn escape_plant(s: &str) -> String {
    s.replace('"', "''").replace('\n', " ")
}
