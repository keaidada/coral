// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Translate PostgreSQL / GaussDB `to_char`/`to_date`/`to_timestamp` format
//! strings into Spark's Java `DateTimeFormatter` tokens.
//!
//! Mirrors `ParseTreeBuilder.translatePgDateFormat` in the Java tree verbatim —
//! same longest-match ordering, same token map.
//!
//! PG vs Spark differences this function rewrites:
//!
//! | PG token | Spark token | Meaning                  |
//! |----------|-------------|--------------------------|
//! | `YYYY`   | `yyyy`      | 4-digit year             |
//! | `YY`     | `yy`        | 2-digit year             |
//! | `MON`    | `MMM`       | abbreviated month name   |
//! | `MM`     | `MM`        | numeric month (same)     |
//! | `MI`     | `mm`        | minute                   |
//! | `DY`     | `EEE`       | day-of-week abbreviation |
//! | `DD`     | `dd`        | day of month             |
//! | `HH24`   | `HH`        | 24-hour                  |
//! | `HH12`   | `hh`        | 12-hour                  |
//! | `HH`     | `hh`        | 12-hour (PG default)     |
//! | `SS`     | `ss`        | seconds                  |
//! | `AM`/`PM`| `a`         | am/pm marker             |
//! | `FF<n>`  | `S`*n       | sub-second precision     |

/// Translate a PG/GaussDB format string into its Spark equivalent.
///
/// Unrecognized characters are copied verbatim (including `/ - : .` and any
/// literal letters that happen not to be a token). Matching is case-insensitive
/// on the PG side but produces the canonical Spark casing (`yyyy` vs `YYYY`).
pub fn translate_pg_date_format(pg: &str) -> String {
    let bytes = pg.as_bytes();
    let mut out = String::with_capacity(pg.len());
    let mut i = 0;
    while i < bytes.len() {
        // longest-match first; use uppercase view for comparison
        let remaining = &pg[i..];
        let upper: String = remaining.chars().take(4).collect::<String>().to_uppercase();

        let (replaced, consumed): (Option<String>, usize) = if upper.starts_with("YYYY") {
            (Some("yyyy".into()), 4)
        } else if upper.starts_with("YY") {
            (Some("yy".into()), 2)
        } else if upper.starts_with("MON") {
            (Some("MMM".into()), 3)
        } else if upper.starts_with("MI") {
            (Some("mm".into()), 2)
        } else if upper.starts_with("MM") {
            (Some("MM".into()), 2)
        } else if upper.starts_with("DY") {
            (Some("EEE".into()), 2)
        } else if upper.starts_with("DD") {
            (Some("dd".into()), 2)
        } else if upper.starts_with("HH24") {
            (Some("HH".into()), 4)
        } else if upper.starts_with("HH12") {
            (Some("hh".into()), 4)
        } else if upper.starts_with("HH") {
            (Some("hh".into()), 2)
        } else if upper.starts_with("SS") {
            (Some("ss".into()), 2)
        } else if upper.starts_with("AM") || upper.starts_with("PM") {
            (Some("a".into()), 2)
        } else if upper.starts_with("FF") {
            // FF<digit>  -> that many 'S' chars, default 3 for plain FF.
            let next = upper.as_bytes().get(2).copied();
            if let Some(d) = next {
                if d.is_ascii_digit() {
                    let n = (d - b'0') as usize;
                    (Some("S".repeat(n)), 3)
                } else {
                    (Some("SSS".into()), 2)
                }
            } else {
                (Some("SSS".into()), 2)
            }
        } else {
            (None, 0)
        };

        if let Some(spark) = replaced {
            out.push_str(&spark);
            i += consumed;
        } else {
            // Non-token byte: copy as-is. Safe because we're advancing by 1
            // byte; PG format strings are ASCII in practice.
            out.push(bytes[i] as char);
            i += 1;
        }
    }
    out
}

/// Heuristic: does the format string contain any date/time token? Used by
/// `to_char(expr, fmt)` to decide whether to rewrite into `date_format` (dates
/// and timestamps) vs pass through unchanged (numeric formatting like
/// `to_char(x, 'fm999.99')`).
pub fn contains_date_format_token(fmt: &str) -> bool {
    let u = fmt.to_uppercase();
    const TOKENS: &[&str] = &[
        "YYYY", "YY", "MM", "DD", "HH", "MI", "SS", "MON", "DY", "FF", "AM", "PM",
    ];
    TOKENS.iter().any(|t| u.contains(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_date_tokens() {
        assert_eq!(translate_pg_date_format("YYYY-MM-DD"), "yyyy-MM-dd");
    }

    #[test]
    fn timestamp_with_hh24_and_mi() {
        assert_eq!(
            translate_pg_date_format("YYYY-MM-DD HH24:MI:SS"),
            "yyyy-MM-dd HH:mm:ss"
        );
    }

    #[test]
    fn ff_with_digits() {
        assert_eq!(translate_pg_date_format("HH24:MI:SS.FF3"), "HH:mm:ss.SSS");
        assert_eq!(
            translate_pg_date_format("HH24:MI:SS.FF6"),
            "HH:mm:ss.SSSSSS"
        );
    }

    #[test]
    fn ff_without_digit_defaults_to_3() {
        assert_eq!(translate_pg_date_format("FF"), "SSS");
    }

    #[test]
    fn non_token_passthrough() {
        assert_eq!(translate_pg_date_format("at YYYY-MM"), "at yyyy-MM");
    }

    #[test]
    fn detects_tokens() {
        assert!(contains_date_format_token("YYYY-MM-DD"));
        assert!(contains_date_format_token("HH:MI"));
        assert!(!contains_date_format_token("fm999.99"));
    }

    #[test]
    fn case_insensitive_input() {
        // PG is case-insensitive on input but our output is canonical.
        assert_eq!(translate_pg_date_format("yyyy"), "yyyy");
        assert_eq!(translate_pg_date_format("YYYY"), "yyyy");
    }
}
