// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum PigError {
    #[error("parse error: {0}")]
    Parse(#[from] sqlparser::parser::ParserError),

    #[error("no SELECT found in input")]
    NoSelect,

    #[error("unsupported shape: {0}")]
    Unsupported(String),
}

pub type Result<T, E = PigError> = std::result::Result<T, E>;
