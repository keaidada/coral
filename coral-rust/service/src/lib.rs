// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! HTTP service exposing the coral-rust translator.
//!
//! Rust port of Java `coral-service` (Spring Boot → axum). Mirrors the
//! Java endpoint paths so clients written against the Java tree work
//! without modification:
//!
//! | Method | Path                                  | Purpose                               |
//! |--------|---------------------------------------|---------------------------------------|
//! | POST   | `/api/translations/translate`         | SQL → SQL (Spark or Trino)            |
//! | POST   | `/api/translations/validate`          | SQL → validation issues (extension)   |
//! | POST   | `/api/visualizations/generategraphs`  | AST → DOT / PlantUML source (ID only) |
//! | GET    | `/api/visualizations/{id}`            | Retrieve graph source by ID           |
//! | GET    | `/api/functions`                      | Full function registry (JSON)         |
//! | GET    | `/api/health`                         | Liveness probe                        |
//!
//! The library is decoupled from the binary (`coral-service` crate
//! exposes [`router`] so integration tests and embedders can use it
//! directly without spawning a subprocess).

pub mod handlers;
pub mod models;
pub mod state;

use axum::{routing::get, routing::post, Router};
use tower_http::cors::CorsLayer;

use crate::state::AppState;

/// Build the top-level axum Router. Callers bind it to a TCP port
/// themselves (see `main.rs` for the canonical `axum::serve` invocation)
/// or wire it into their own Router tree for testing.
pub fn router() -> Router {
    let state = AppState::new();
    Router::new()
        .route("/api/health", get(handlers::health))
        .route(
            "/api/translations/translate",
            post(handlers::translate),
        )
        .route(
            "/api/translations/validate",
            post(handlers::validate),
        )
        .route(
            "/api/visualizations/generategraphs",
            post(handlers::generate_graphs),
        )
        .route(
            "/api/visualizations/:id",
            get(handlers::get_visualization),
        )
        .route("/api/functions", get(handlers::list_functions))
        .with_state(state)
        .layer(CorsLayer::permissive())
}
