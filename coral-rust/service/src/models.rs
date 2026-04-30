// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Request / response shapes for the HTTP surface.
//!
//! Field names mirror Java `coral-service`'s JSON schema so existing
//! clients don't need to change their payloads. The camelCase vs
//! snake_case convention matches Spring Boot's default output, i.e.
//! `sourceLanguage` / `targetLanguage`, not `source_language`.

use serde::{Deserialize, Serialize};

/// POST /api/translations/translate
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateRequest {
    /// Source SQL text.
    pub query: String,

    /// One of `hive`, `spark`, `trino`, `gaussdb`. Accepted but
    /// currently ignored by coral-rust (the PostgreSQL dialect of
    /// sqlparser-rs parses all four variants we care about). Kept
    /// in the schema for Java-client compatibility.
    #[serde(default)]
    pub source_language: Option<String>,

    /// One of `spark` (default) or `trino` / `presto`.
    #[serde(default)]
    pub target_language: Option<String>,

    /// Reserved for future rewrites (`INCREMENTAL`, `DATAMASKING`).
    /// `NONE` or omitted → plain translation.
    #[serde(default)]
    pub rewrite_type: Option<String>,
}

/// POST /api/translations/translate
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslateResponse {
    /// The translated SQL, or empty on error.
    pub translated: String,
    /// Either "spark" or "trino".
    pub target: String,
    /// Soft validation issues (currently always empty unless a catalog
    /// was provided — the HTTP surface doesn't thread one yet).
    #[serde(default)]
    pub issues: Vec<String>,
    /// Non-null when translation failed. `translated` will be empty.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// POST /api/translations/validate
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateRequest {
    pub query: String,
}

/// POST /api/translations/validate
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ValidateResponse {
    pub parses: bool,
    pub statement_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parse_error: Option<String>,
}

/// POST /api/visualizations/generategraphs
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizeRequest {
    pub query: String,
    /// Accepted values: `dot`, `plantuml`. Default = `dot`.
    #[serde(default)]
    pub format: Option<String>,
}

/// POST /api/visualizations/generategraphs
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualizeResponse {
    /// UUID clients pass to GET /api/visualizations/{id} to fetch the
    /// rendered graph source.
    pub graph_id: String,
    pub format: String,
}

/// GET /api/functions
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionListResponse {
    pub total: usize,
    pub entries: Vec<FunctionEntryDto>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FunctionEntryDto {
    pub name: String,
    pub disposition: String,
    pub category: String,
    pub notes: String,
}

/// GET /api/health
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthResponse {
    pub status: &'static str,
    pub version: &'static str,
}
