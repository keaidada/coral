// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Text-level pre-processors that run BEFORE sqlparser-rs sees the input.
//!
//! sqlparser-rs cannot parse two GaussDB/Oracle constructs in its PostgreSQL
//! dialect: (1) `START WITH ... CONNECT BY` hierarchical queries, and (2)
//! Oracle `(+)` outer-join syntax. Both are rewritten here to standards-based
//! equivalents so the downstream parser + AST rewriters see only portable SQL.

pub mod connect_by;
pub mod oracle_outer_join;

pub use connect_by::rewrite_connect_by;
pub use oracle_outer_join::rewrite_oracle_outer_join;

/// Run every text preprocessor in order. Used by [`crate::translate`].
pub fn preprocess(sql: &str) -> String {
    let s = rewrite_connect_by(sql);
    rewrite_oracle_outer_join(&s)
}
