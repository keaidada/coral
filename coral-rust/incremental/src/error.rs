// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum IncrementalError {
    #[error("parse error: {0}")]
    Parse(#[from] sqlparser::parser::ParserError),

    #[error("too many base-table references ({0}); incremental expansion is capped at {max} tables", max = crate::MAX_TABLES)]
    TooManyTables(usize),

    #[error("no base tables found in input — nothing to make incremental")]
    NoTables,
}

pub type Result<T, E = IncrementalError> = std::result::Result<T, E>;
