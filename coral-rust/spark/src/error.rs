// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum SparkError {
    #[error("parse error: {0}")]
    Parse(#[from] sqlparser::parser::ParserError),

    #[error("translate error: {0}")]
    Translate(#[from] coral_core::CoralError),

    #[error("schema error: {0}")]
    Schema(#[from] coral_schema::SchemaError),

    #[error("plan analysis error: {0}")]
    Plan(String),
}

pub type Result<T, E = SparkError> = std::result::Result<T, E>;
