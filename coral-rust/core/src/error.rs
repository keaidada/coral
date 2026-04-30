// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

use thiserror::Error;

/// Errors that can occur during GaussDB -> Spark SQL translation.
#[derive(Debug, Error)]
pub enum CoralError {
    /// The input SQL failed to parse as GaussDB (PostgreSQL-compatible) SQL.
    #[error("parse error: {0}")]
    Parse(String),

    /// A rewrite rule encountered a construct it cannot translate safely.
    #[error("unsupported construct: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, CoralError>;

impl From<sqlparser::parser::ParserError> for CoralError {
    fn from(e: sqlparser::parser::ParserError) -> Self {
        Self::Parse(e.to_string())
    }
}
