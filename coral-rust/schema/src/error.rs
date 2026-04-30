// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SchemaError {
    #[error("parse error: {0}")]
    Parse(#[from] sqlparser::parser::ParserError),

    #[error("no SELECT statement found — expected CREATE VIEW ... AS SELECT or plain SELECT")]
    NoSelect,

    #[error("unsupported SELECT shape: {0}")]
    Unsupported(String),

    #[error("catalog error: {0}")]
    Catalog(String),

    #[error("json serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T, E = SchemaError> = std::result::Result<T, E>;
