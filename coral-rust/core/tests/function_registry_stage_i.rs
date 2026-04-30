// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Stage I coverage tests: Hive built-ins that weren't in the initial
//! 198-entry port but live in Java `StaticHiveFunctionRegistry`. Also
//! spot-checks the LinkedIn-FQN UDF entries (registered as known but
//! UnsupportedBySpark).

use coral_core::{function_catalog, translate, Disposition};

#[test]
fn registry_total_matches_java_parity_target() {
    // Java StaticHiveFunctionRegistry has ~296 short+FQN names (196
    // short + ~100 FQN). coral-rust must be ≥302 to claim "full Hive
    // function parity".
    let n = function_catalog::registry().len();
    assert!(
        n >= 302,
        "registry size should be ≥302, got {n} — Stage I missing entries?"
    );
}

// ---------- newly covered short names ----------

#[test]
fn control_flow_keywords_are_registered() {
    for name in ["case", "when", "in", "between"] {
        let e = function_catalog::lookup(name).unwrap_or_else(|| panic!("missing: {name}"));
        assert_eq!(e.disposition, Disposition::Passthrough, "{name}");
    }
}

#[test]
fn null_family_is_registered() {
    for name in [
        "nullif",
        "isnull",
        "isnotnull",
        "tok_isnull",
        "tok_isnotnull",
    ] {
        assert!(function_catalog::is_known(name), "{name} not registered");
    }
}

#[test]
fn window_family_is_registered() {
    for name in [
        "cume_dist",
        "percent_rank",
        "first_value",
        "last_value",
        "nth_value",
        "lag",
        "lead",
    ] {
        let e = function_catalog::lookup(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(e.disposition, Disposition::Passthrough, "{name}");
    }
}

#[test]
fn variance_family_is_registered() {
    for name in [
        "variance",
        "var_pop",
        "var_samp",
        "stddev",
        "stddev_pop",
        "stddev_samp",
    ] {
        assert!(function_catalog::is_known(name), "{name}");
    }
}

#[test]
fn replace_translate3_rlike_regexp_registered() {
    for name in ["replace", "translate3", "rlike", "regexp"] {
        assert!(function_catalog::is_known(name), "{name}");
    }
}

#[test]
fn reflect_java_method_marked_unsupported() {
    for name in ["reflect", "java_method", "generic_project"] {
        let e = function_catalog::lookup(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            e.disposition,
            Disposition::UnsupportedBySpark,
            "{name} should be UnsupportedBySpark (no Spark analog)"
        );
    }
}

#[test]
fn strpos_renamed_to_instr_for_spark() {
    // GaussDB / PG / Trino expose `strpos(haystack, needle) -> INT`.
    // Spark's equivalent is INSTR with the same argument order, so the
    // translator MUST rename (not reorder). Defends against accidentally
    // regressing to passthrough (which would emit invalid Spark SQL)
    // or to a reorder (which would flip semantics).
    let e = function_catalog::lookup("strpos").unwrap_or_else(|| panic!("strpos missing"));
    match e.disposition {
        Disposition::Rename(new) => assert_eq!(new, "INSTR"),
        other => panic!("expected Rename(INSTR), got {other:?}"),
    }
    let got = translate("SELECT strpos(s, 'A') FROM t").unwrap();
    assert!(got.contains("INSTR(s, 'A')"), "{got}");
    assert!(!got.to_uppercase().contains("STRPOS("), "{got}");
}

#[test]
fn timestamp_from_unixtime_renamed() {
    let e = function_catalog::lookup("timestamp_from_unixtime").unwrap();
    match e.disposition {
        Disposition::Rename(new) => {
            assert_eq!(new, "TIMESTAMP_SECONDS");
        }
        other => panic!("expected rename, got {other:?}"),
    }
}

#[test]
fn from_utf8_renamed_to_decode() {
    let e = function_catalog::lookup("from_utf8").unwrap();
    match e.disposition {
        Disposition::Rename(new) => {
            assert_eq!(new, "DECODE");
        }
        other => panic!("expected rename, got {other:?}"),
    }
}

// ---------- LinkedIn FQN entries ----------

#[test]
fn linkedin_fqn_udfs_registered_as_unsupported() {
    // Spot-check: a dozen names across the Dali / Orbit / Groot / TSCP
    // families should all be Known + UnsupportedBySpark.
    for name in [
        "com.linkedin.dali.udf.date.hive.dateformattoepoch",
        "com.linkedin.dali.udf.sanitize.hive.sanitize",
        "com.linkedin.dali.udf.monarch.urngenerator",
        "com.linkedin.dali.view.udf.entityhandles.getidfromurn",
        "com.linkedin.dali.views.premium.udf.getfamily",
        "com.linkedin.dali.views.search.udf.getverticaludf",
        "com.linkedin.dwh.udf.profile.getprofileurl",
        "com.linkedin.groot.runtime.udf.spark.hasmemberconsentudf",
        "com.linkedin.tscp.reporting.dali.udfs.activityid",
        "com.linkedin.udfs.standard.hive.obfuscateall",
        "com.linkedin.vector.daliview.udf.presentdatatype",
        "udfs.seoreferrertrkudf",
    ] {
        let e = function_catalog::lookup(name).unwrap_or_else(|| panic!("missing {name}"));
        assert_eq!(
            e.disposition,
            Disposition::UnsupportedBySpark,
            "{name} should be UnsupportedBySpark"
        );
    }
}

// ---------- rewriter still behaves identically for unchanged names ----------

#[test]
fn legacy_rewrites_still_work_after_stage_i() {
    // NVL / STRING_AGG / POSITION / BOOL_AND etc. were present before
    // Stage I — make sure we didn't break them by appending rows.
    for (input, want) in [
        ("SELECT NVL(a, b) FROM t", "COALESCE(a, b)"),
        ("SELECT BOOL_AND(x) FROM t", "EVERY(x)"),
        ("SELECT BOOL_OR(x) FROM t", "SOME(x)"),
        ("SELECT ARRAY_AGG(x) FROM t", "COLLECT_LIST(x)"),
        (
            "SELECT STRING_AGG(x, ',') FROM t",
            "CONCAT_WS(',', COLLECT_LIST(x))",
        ),
    ] {
        let got = translate(input).unwrap();
        assert!(got.contains(want), "input={input} got={got}");
    }
}
