// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Server-wide shared state.
//!
//! Holds the in-memory visualization cache (generated graph sources
//! keyed by UUID). Java `coral-service` keeps rendered PNG/SVG bytes
//! here; the Rust port returns graph SOURCE (DOT / PlantUML text) and
//! lets the client render it — keeping the server zero-dep on graphviz
//! or plantuml.jar.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub struct AppState {
    /// graph_id → (format, source text). Stored under a single mutex
    /// because generate/list/get are all short critical sections.
    pub graphs: Arc<Mutex<HashMap<String, GraphPayload>>>,
}

#[derive(Clone, Debug)]
pub struct GraphPayload {
    pub format: String,
    pub source: String,
}

impl AppState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn store(&self, id: String, payload: GraphPayload) {
        if let Ok(mut g) = self.graphs.lock() {
            g.insert(id, payload);
        }
    }

    pub fn get(&self, id: &str) -> Option<GraphPayload> {
        self.graphs
            .lock()
            .ok()
            .and_then(|g| g.get(id).cloned())
    }
}
