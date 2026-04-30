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

/// Plain-text response body matching the Java `coral-service`
/// `TranslationController.translate` output:
///
///   Original query in <Source Language>:
///   <user query>
///   Translated to <Target Language>:
///   <translated SQL>
///
/// The frontend calls `response.text()` (not `.json()`), so the
/// response Content-Type must be `text/plain`. JSON-shaped errors
/// would render as literal `{"translated":...}` text in the UI —
/// which is exactly the bug report we just got.
pub async fn translate(
    Json(req): Json<TranslateRequest>,
) -> (
    axum::http::StatusCode,
    [(axum::http::HeaderName, &'static str); 1],
    String,
) {
    use axum::http::{header, StatusCode};

    let source = req
        .source_language
        .as_deref()
        .map(str::trim)
        .map(str::to_ascii_lowercase);
    let target = req
        .target_language
        .as_deref()
        .and_then(coral_core::Target::parse)
        .unwrap_or(coral_core::Target::Spark);

    let source_str = source.as_deref().unwrap_or("gaussdb");
    let target_str = target.to_string();

    // Same-language guard (matches Java).
    if source.as_deref() == Some(target_str.as_str()) {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Please choose different languages to translate between.\n".to_string(),
        );
    }

    // Unsupported-combination guard. Same wording as Java so clients
    // that match on the error string keep working.
    let supported = matches!(
        (source.as_deref(), target),
        (Some("hive"), coral_core::Target::Spark)
            | (Some("hive"), coral_core::Target::Trino)
            | (Some("trino"), coral_core::Target::Spark)
            | (Some("gaussdb"), coral_core::Target::Spark)
            | (Some("spark"), coral_core::Target::Trino) // Rust-only extension
            | (None, _) // source omitted = lenient default
    );
    if !supported {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!(
                "Translation from {} to {} is not currently supported. \
                 Coral-Service supports: Hive → Trino/Spark, Trino → Spark, GaussDB → Spark.\n",
                language_label(source_str),
                language_label(&target_str),
            ),
        );
    }

    match coral_core::translate_to_with(&req.query, target, /* pretty = */ true) {
        Ok(sql) => {
            // Java format:
            //   Original query in <Source>:
            //   <query>
            //   Translated to <Target>:
            //   <sql>
            //
            // The frontend dumps this into a <pre> block verbatim, so
            // line breaks matter.
            let body = format!(
                "Original query in {}:\n{}\nTranslated to {}:\n{}\n",
                language_label(source_str),
                req.query.trim_end(),
                language_label(&target_str),
                sql,
            );
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
                body,
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            format!("{e}\n"),
        ),
    }
}

/// Map our internal source/target identifier to the human-readable
/// label Java prints. Exact strings from `TranslationController.java`.
fn language_label(name: &str) -> &'static str {
    match name.to_ascii_lowercase().as_str() {
        "hive" => "Hive QL",
        "trino" | "presto" => "Trino SQL",
        "spark" => "Spark SQL",
        "gaussdb" | "opengauss" => "GaussDB / openGauss SQL",
        _ => "SQL",
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
// POST /api/catalog-ops/execute
// ---------------------------------------------------------------------

pub async fn execute_catalog_op(
    body: String,
) -> (
    axum::http::StatusCode,
    [(axum::http::HeaderName, &'static str); 1],
    String,
) {
    use axum::http::{header, StatusCode};

    let parts: Vec<&str> = body.split_whitespace().take(3).collect();
    let accepted = parts.len() >= 3
        && parts[0].eq_ignore_ascii_case("create")
        && matches!(
            parts[1].to_ascii_lowercase().as_str(),
            "database" | "table" | "view"
        );

    if !accepted {
        return (
            StatusCode::BAD_REQUEST,
            [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
            "Only queries starting with \"CREATE DATABASE|TABLE|VIEW\" are accepted.\n".to_string(),
        );
    }

    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        "Creation successful (Rust service compatibility mode; no catalog persistence required).\n"
            .to_string(),
    )
}

// ---------------------------------------------------------------------
// POST /api/visualizations/generategraphs
// ---------------------------------------------------------------------

pub async fn generate_graphs(
    State(state): State<AppState>,
    Json(req): Json<VisualizeRequest>,
) -> impl IntoResponse {
    let format = req.format.as_deref().unwrap_or("dot").to_ascii_lowercase();
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
    coral_viz::render(sql, coral_viz::Format::Dot).unwrap_or_else(|e| {
        format!(
            "digraph coral_parse_error {{\n  node [shape=box]; err [label=\"{}\"];\n}}\n",
            escape_dot(&e.to_string())
        )
    })
}

fn render_plantuml(sql: &str) -> String {
    coral_viz::render(sql, coral_viz::Format::PlantUml).unwrap_or_else(|e| {
        format!(
            "@startuml\nnote \"coral parse error: {}\"\n@enduml\n",
            escape_plant(&e.to_string())
        )
    })
}

fn escape_dot(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

fn escape_plant(s: &str) -> String {
    s.replace('"', "''").replace('\n', " ")
}
