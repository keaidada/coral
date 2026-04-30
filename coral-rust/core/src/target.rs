// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! The output dialect the translator emits for.
//!
//! All translation pipelines in `coral-core` parse GaussDB / openGauss /
//! Hive-flavored input the same way and run the same structural rewrites
//! (DISTINCT ON → ROW_NUMBER, CONNECT BY → WITH RECURSIVE, (+) → LEFT JOIN).
//! They diverge only in:
//!
//!   1. **Function mappings** — Spark and Trino have different names for
//!      roughly 20 built-ins (NVL vs COALESCE, ARRAY_CONTAINS vs CONTAINS,
//!      RAND vs RANDOM, GET_JSON_OBJECT vs JSON_EXTRACT, etc.).
//!   2. **Type renaming** — Trino uses `REAL` where Spark uses `FLOAT`,
//!      `VARBINARY` where Spark uses `BINARY`.
//!   3. **Syntactic quirks** — LATERAL UNNEST, OFFSET/FETCH vs LIMIT OFFSET,
//!      identifier quoting (`"`) vs backticks.
//!
//! The `Target` enum threads through [`rewrite::apply_all_for_target`] so
//! the user-facing `translate()` / `translate_to_trino()` entry points can
//! share almost all of their implementation.

/// Which SQL dialect the translator should emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Target {
    /// Spark SQL (default). Equivalent to what the Java `coral-gaussdb-spark`
    /// module produces.
    Spark,
    /// Trino SQL. Equivalent to what Java `coral-trino` produces.
    Trino,
}

impl Target {
    /// Parse a target from a CLI-style string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "spark" => Some(Target::Spark),
            "trino" | "presto" => Some(Target::Trino),
            _ => None,
        }
    }

    /// Human-readable name for logs / `--help`.
    pub fn as_str(self) -> &'static str {
        match self {
            Target::Spark => "spark",
            Target::Trino => "trino",
        }
    }
}

impl std::fmt::Display for Target {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
