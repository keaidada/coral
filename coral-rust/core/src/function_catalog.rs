// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Exhaustive known-function registry.
//!
//! For each Hive / GaussDB function we know about, records what the
//! FunctionRewriter does with it. 135 Hive names from
//! `StaticHiveFunctionRegistry.java` + ~30 GaussDB-specific names from
//! `ParseTreeBuilder.visitFunctionCall`, merged.
//!
//! ## What "registered" means here
//!
//! Spark SQL natively implements the vast majority of Hive's built-in
//! functions with identical signatures and names, so the catalog-aware
//! answer for most of them is **passthrough**: keep the function call
//! unchanged. The registry records this decision so:
//!
//!   - Users can `cargo run -- --list-functions` to see what's covered.
//!   - We can warn loudly if a GaussDB/Oracle-ism we've seen is likely to
//!     break under Spark without a rewrite.
//!   - Anyone contributing new rewrites sees the existing table and picks
//!     the right category.
//!
//! The actual rewrite logic still lives in `rewrite/functions.rs`. This
//! module is declarative-only — no rewriting happens here.

use std::collections::HashMap;
use std::sync::OnceLock;

/// What the translator does with a given function call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// Emit the call unchanged. Spark natively supports this name with
    /// compatible semantics.
    Passthrough,
    /// Rename the function; argument layout is preserved. Recorded in the
    /// comment field of the registry entry.
    Rename(&'static str),
    /// The rewriter changes the call shape (e.g. arg order, wraps in
    /// another function, expands into a CASE expression). See the
    /// implementation in `rewrite/functions.rs` — the registry just
    /// documents the existence of the rewrite.
    CustomRewrite,
    /// Known function, but Spark does NOT have a direct equivalent. The
    /// translator passes it through anyway; users will get a runtime error
    /// from Spark unless they've registered the UDF themselves.
    UnsupportedBySpark,
}

/// An entry in the registry.
#[derive(Debug, Clone, Copy)]
pub struct FunctionEntry {
    pub lower_name: &'static str,
    pub disposition: Disposition,
    pub category: Category,
    /// Short human-readable note explaining the decision. Shown in
    /// `--list-functions` output.
    pub notes: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Category {
    Aggregate,
    String,
    Math,
    DateTime,
    Collection,
    Json,
    Hash,
    Bitwise,
    Window,
    Cast,
    Null,
    Conditional,
    Xpath,
    Udtf,
    Context,
    GaussDbSpecific,
    Misc,
}

/// Return the registry as a map. Built once on first access.
pub fn registry() -> &'static HashMap<&'static str, FunctionEntry> {
    static REGISTRY: OnceLock<HashMap<&'static str, FunctionEntry>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let mut m = HashMap::new();
        for e in ENTRIES {
            m.insert(e.lower_name, *e);
        }
        m
    })
}

/// True iff `name` (case-insensitively) is recognized.
pub fn is_known<S: AsRef<str>>(name: S) -> bool {
    registry().contains_key(name.as_ref().to_ascii_lowercase().as_str())
}

/// Look up an entry by name (case-insensitive).
pub fn lookup<S: AsRef<str>>(name: S) -> Option<FunctionEntry> {
    registry()
        .get(name.as_ref().to_ascii_lowercase().as_str())
        .copied()
}

/// Every entry currently in the registry. Ordered roughly by category for
/// readability.
const ENTRIES: &[FunctionEntry] = &[
    // ============================================================
    // Aggregates
    // ============================================================
    e("sum", Passthrough, Aggregate, "Spark-native"),
    e("avg", Passthrough, Aggregate, "Spark-native"),
    e("min", Passthrough, Aggregate, "Spark-native"),
    e("max", Passthrough, Aggregate, "Spark-native"),
    e("count", Passthrough, Aggregate, "Spark-native"),
    e("collect_list", Passthrough, Aggregate, "Spark-native"),
    e("collect_set", Passthrough, Aggregate, "Spark-native"),
    e(
        "array_agg",
        Disposition::Rename("COLLECT_LIST"),
        Aggregate,
        "Rewritten by FunctionRewriter",
    ),
    e(
        "string_agg",
        Disposition::CustomRewrite,
        Aggregate,
        "-> CONCAT_WS(sep, COLLECT_LIST(x))",
    ),
    e(
        "bool_and",
        Disposition::Rename("EVERY"),
        Aggregate,
        "Rewritten",
    ),
    e(
        "bool_or",
        Disposition::Rename("SOME"),
        Aggregate,
        "Rewritten",
    ),
    e(
        "context_ngrams",
        Passthrough,
        Aggregate,
        "Spark-native (hive-compat)",
    ),
    e(
        "ngrams",
        Passthrough,
        Aggregate,
        "Spark-native (hive-compat)",
    ),
    e("sentences", Passthrough, Aggregate, "Spark-native"),
    // ============================================================
    // Window / ranking
    // ============================================================
    e("row_number", Passthrough, Window, "Spark-native"),
    e("rank", Passthrough, Window, "Spark-native"),
    e("dense_rank", Passthrough, Window, "Spark-native"),
    // ============================================================
    // Null handling / conditional
    // ============================================================
    e("coalesce", Passthrough, Null, "Spark-native"),
    e("nvl", Disposition::Rename("COALESCE"), Null, "Rewritten"),
    e(
        "nvl2",
        Disposition::CustomRewrite,
        Null,
        "-> CASE WHEN a IS NOT NULL THEN b ELSE c END",
    ),
    e(
        "decode",
        Disposition::CustomRewrite,
        Null,
        "-> CASE WHEN x = k1 THEN v1 ... END",
    ),
    e("greatest", Passthrough, Conditional, "Spark-native"),
    e("least", Passthrough, Conditional, "Spark-native"),
    e("if", Passthrough, Conditional, "Spark-native"),
    // ============================================================
    // String functions
    // ============================================================
    e("ascii", Passthrough, String, "Spark-native"),
    e("base64", Passthrough, String, "Spark-native"),
    e("unbase64", Passthrough, String, "Spark-native"),
    e(
        "character_length",
        Passthrough,
        String,
        "Spark-native (length)",
    ),
    e("chr", Passthrough, String, "Spark-native"),
    e("concat", Passthrough, String, "Spark-native"),
    e("concat_ws", Passthrough, String, "Spark-native"),
    e("elt", Passthrough, String, "Spark-native"),
    e("encode", Passthrough, String, "Spark-native"),
    e("field", Passthrough, String, "Spark-native"),
    e("find_in_set", Passthrough, String, "Spark-native"),
    e("format_number", Passthrough, String, "Spark-native"),
    e("initcap", Passthrough, String, "Spark-native"),
    e("instr", Passthrough, String, "Spark-native"),
    e(
        "strpos",
        Disposition::Rename("INSTR"),
        String,
        "GaussDB/PG/Trino strpos(s, p) -> Spark INSTR(s, p) (same arg order)",
    ),
    e("length", Passthrough, String, "Spark-native"),
    e("levenshtein", Passthrough, String, "Spark-native"),
    e("locate", Passthrough, String, "Spark-native"),
    e("lower", Passthrough, String, "Spark-native"),
    e("lcase", Disposition::Rename("LOWER"), String, "Hive alias"),
    e("upper", Passthrough, String, "Spark-native"),
    e("ucase", Disposition::Rename("UPPER"), String, "Hive alias"),
    e("lpad", Passthrough, String, "Spark-native"),
    e("rpad", Passthrough, String, "Spark-native"),
    e("ltrim", Passthrough, String, "Spark-native"),
    e("rtrim", Passthrough, String, "Spark-native"),
    e("trim", Passthrough, String, "Spark-native"),
    e("octet_length", Passthrough, String, "Spark-native"),
    e("parse_url", Passthrough, String, "Spark-native"),
    e(
        "position",
        Disposition::CustomRewrite,
        String,
        "-> INSTR(haystack, needle) (arg swap)",
    ),
    e("printf", Passthrough, String, "Spark-native"),
    e("repeat", Passthrough, String, "Spark-native"),
    e("reverse", Passthrough, String, "Spark-native"),
    e("regexp_extract", Passthrough, String, "Spark-native"),
    e("regexp_replace", Passthrough, String, "Spark-native"),
    e(
        "regexp_substr",
        Disposition::CustomRewrite,
        String,
        "-> REGEXP_EXTRACT(s, p, 0)",
    ),
    e("soundex", Passthrough, String, "Spark-native"),
    e("space", Passthrough, String, "Spark-native"),
    e("split", Passthrough, String, "Spark-native"),
    e("split_part", Passthrough, String, "Spark 3.3+"),
    e("str_to_map", Passthrough, String, "Spark-native"),
    e(
        "substr",
        Disposition::Rename("SUBSTRING"),
        String,
        "Rewritten",
    ),
    e("substring", Passthrough, String, "Spark-native"),
    e("substring_index", Passthrough, String, "Spark-native"),
    e("translate", Passthrough, String, "Spark-native"),
    // ============================================================
    // Math
    // ============================================================
    e("abs", Passthrough, Math, "Spark-native"),
    e("acos", Passthrough, Math, "Spark-native"),
    e("asin", Passthrough, Math, "Spark-native"),
    e("atan", Passthrough, Math, "Spark-native"),
    e("atan2", Passthrough, Math, "Spark-native"),
    e("bround", Passthrough, Math, "Spark-native"),
    e("cbrt", Passthrough, Math, "Spark-native"),
    e("ceil", Passthrough, Math, "Spark-native"),
    e("ceiling", Disposition::Rename("CEIL"), Math, "Hive alias"),
    e("conv", Passthrough, Math, "Spark-native"),
    e("cos", Passthrough, Math, "Spark-native"),
    e("cosh", Passthrough, Math, "Spark-native"),
    e("degrees", Passthrough, Math, "Spark-native"),
    e("e", Passthrough, Math, "Spark-native"),
    e("exp", Passthrough, Math, "Spark-native"),
    e("factorial", Passthrough, Math, "Spark-native"),
    e("floor", Passthrough, Math, "Spark-native"),
    e("hex", Passthrough, Math, "Spark-native"),
    e("ln", Passthrough, Math, "Spark-native"),
    e("log", Passthrough, Math, "Spark-native"),
    e("log10", Passthrough, Math, "Spark-native"),
    e("log2", Passthrough, Math, "Spark-native"),
    e("mod", Disposition::CustomRewrite, Math, "-> a % b"),
    e("negative", Passthrough, Math, "Spark-native"),
    e("pi", Passthrough, Math, "Spark-native"),
    e("pmod", Passthrough, Math, "Spark-native"),
    e("positive", Passthrough, Math, "Spark-native"),
    e("pow", Disposition::Rename("POWER"), Math, "Hive alias"),
    e("power", Passthrough, Math, "Spark-native"),
    e("radians", Passthrough, Math, "Spark-native"),
    e("rand", Passthrough, Math, "Spark-native"),
    e("random", Disposition::Rename("RAND"), Math, "Rewritten"),
    e("round", Passthrough, Math, "Spark-native"),
    e("sign", Passthrough, Math, "Spark-native"),
    e("sin", Passthrough, Math, "Spark-native"),
    e("sinh", Passthrough, Math, "Spark-native"),
    e("sqrt", Passthrough, Math, "Spark-native"),
    e("tan", Passthrough, Math, "Spark-native"),
    e("tanh", Passthrough, Math, "Spark-native"),
    e("unhex", Passthrough, Math, "Spark-native"),
    e("width_bucket", Passthrough, Math, "Spark 3+"),
    // ============================================================
    // Bitwise
    // ============================================================
    e("shiftleft", Passthrough, Bitwise, "Spark-native"),
    e("shiftright", Passthrough, Bitwise, "Spark-native"),
    e("shiftrightunsigned", Passthrough, Bitwise, "Spark-native"),
    // ============================================================
    // Date & time
    // ============================================================
    e("add_months", Passthrough, DateTime, "Spark-native"),
    e("current_date", Passthrough, DateTime, "Spark-native"),
    e("current_timestamp", Passthrough, DateTime, "Spark-native"),
    e("date", Passthrough, DateTime, "Spark-native"),
    e("date_add", Passthrough, DateTime, "Spark-native"),
    e("date_format", Passthrough, DateTime, "Spark-native"),
    e("date_sub", Passthrough, DateTime, "Spark-native"),
    e("datediff", Passthrough, DateTime, "Spark-native"),
    e("day", Passthrough, DateTime, "Spark-native"),
    e("dayofmonth", Passthrough, DateTime, "Spark-native"),
    e("dayofweek", Passthrough, DateTime, "Spark-native"),
    e("dayofyear", Passthrough, DateTime, "Spark-native"),
    e("from_unixtime", Passthrough, DateTime, "Spark-native"),
    e(
        "from_unixtime_nanos",
        Disposition::UnsupportedBySpark,
        DateTime,
        "No Spark equivalent",
    ),
    e("from_utc_timestamp", Passthrough, DateTime, "Spark-native"),
    e("hour", Passthrough, DateTime, "Spark-native"),
    e("last_day", Passthrough, DateTime, "Spark-native"),
    e("minute", Passthrough, DateTime, "Spark-native"),
    e("month", Passthrough, DateTime, "Spark-native"),
    e("months_between", Passthrough, DateTime, "Spark-native"),
    e("next_day", Passthrough, DateTime, "Spark-native"),
    e(
        "now",
        Disposition::CustomRewrite,
        DateTime,
        "-> CURRENT_TIMESTAMP",
    ),
    e("quarter", Passthrough, DateTime, "Spark-native"),
    e("second", Passthrough, DateTime, "Spark-native"),
    e(
        "sysdate",
        Disposition::CustomRewrite,
        DateTime,
        "-> CURRENT_TIMESTAMP",
    ),
    e(
        "to_date",
        Disposition::CustomRewrite,
        DateTime,
        "Format literal PG->Spark tokens",
    ),
    e(
        "to_timestamp",
        Disposition::CustomRewrite,
        DateTime,
        "Format literal PG->Spark tokens",
    ),
    e(
        "to_char",
        Disposition::CustomRewrite,
        DateTime,
        "Rewrites to DATE_FORMAT for date-ish formats",
    ),
    e("to_unixtime", Passthrough, DateTime, "Spark 3+"),
    e("to_utc_timestamp", Passthrough, DateTime, "Spark-native"),
    e(
        "trunc",
        Disposition::CustomRewrite,
        DateTime,
        "-> DATE_TRUNC for string unit",
    ),
    e("unix_timestamp", Passthrough, DateTime, "Spark-native"),
    e("weekofyear", Passthrough, DateTime, "Spark-native"),
    e("year", Passthrough, DateTime, "Spark-native"),
    e("at_timezone", Passthrough, DateTime, "Spark 3+"),
    e("with_timezone", Passthrough, DateTime, "Spark 3+"),
    // ============================================================
    // Collection (array / map)
    // ============================================================
    e("array", Passthrough, Collection, "Spark-native"),
    e("array_contains", Passthrough, Collection, "Spark-native"),
    e("map", Passthrough, Collection, "Spark-native"),
    e("map_keys", Passthrough, Collection, "Spark-native"),
    e("map_values", Passthrough, Collection, "Spark-native"),
    e("size", Passthrough, Collection, "Spark-native"),
    e("sort_array", Passthrough, Collection, "Spark-native"),
    e("struct", Passthrough, Collection, "Spark-native"),
    e("named_struct", Passthrough, Collection, "Spark-native"),
    e("sequence", Passthrough, Collection, "Spark-native"),
    e(
        "generate_series",
        Disposition::Rename("SEQUENCE"),
        Collection,
        "Rewritten",
    ),
    e("explode", Passthrough, Udtf, "Spark-native"),
    e("posexplode", Passthrough, Udtf, "Spark-native"),
    e("inline", Passthrough, Udtf, "Spark-native"),
    e("stack", Passthrough, Udtf, "Spark-native"),
    // ============================================================
    // JSON
    // ============================================================
    e("get_json_object", Passthrough, Json, "Spark-native"),
    e("json_tuple", Passthrough, Json, "Spark-native"),
    e("to_json", Passthrough, Json, "Spark 2.2+"),
    e("from_json", Passthrough, Json, "Spark 2.2+"),
    // ============================================================
    // Hash
    // ============================================================
    e("crc32", Passthrough, Hash, "Spark-native"),
    e("md5", Passthrough, Hash, "Spark-native"),
    e("sha", Disposition::Rename("SHA1"), Hash, "Hive alias"),
    e("sha1", Passthrough, Hash, "Spark-native"),
    e("sha2", Passthrough, Hash, "Spark-native"),
    e("hash", Passthrough, Hash, "Spark-native"),
    // ============================================================
    // XPath
    // ============================================================
    e("xpath", Passthrough, Xpath, "Spark-native"),
    e("xpath_boolean", Passthrough, Xpath, "Spark-native"),
    e("xpath_double", Passthrough, Xpath, "Spark-native"),
    e("xpath_float", Passthrough, Xpath, "Spark-native"),
    e("xpath_int", Passthrough, Xpath, "Spark-native"),
    e("xpath_long", Passthrough, Xpath, "Spark-native"),
    e("xpath_number", Passthrough, Xpath, "Spark-native"),
    e("xpath_short", Passthrough, Xpath, "Spark-native"),
    e("xpath_string", Passthrough, Xpath, "Spark-native"),
    // ============================================================
    // Casts (the :: operator rewrites; explicit CAST is built-in)
    // ============================================================
    e(
        "binary",
        Passthrough,
        Cast,
        "Spark-native (as CAST AS BINARY)",
    ),
    e("boolean", Passthrough, Cast, "Spark cast helper"),
    e("int", Passthrough, Cast, "Spark cast helper"),
    e("bigint", Passthrough, Cast, "Spark cast helper"),
    e("double", Passthrough, Cast, "Spark cast helper"),
    e("float", Passthrough, Cast, "Spark cast helper"),
    e("smallint", Passthrough, Cast, "Spark cast helper"),
    e("tinyint", Passthrough, Cast, "Spark cast helper"),
    e("string", Passthrough, Cast, "Spark cast helper"),
    // ============================================================
    // Context / session
    // ============================================================
    e("current_user", Passthrough, Context, "Spark-native"),
    e("current_database", Passthrough, Context, "Spark-native"),
    e(
        "logged_in_user",
        Disposition::UnsupportedBySpark,
        Context,
        "Hive-specific",
    ),
    // ============================================================
    // In-file / misc
    // ============================================================
    e(
        "in_file",
        Disposition::UnsupportedBySpark,
        Misc,
        "Hive-specific",
    ),
    e(
        "extract_union",
        Disposition::UnsupportedBySpark,
        Misc,
        "Hive union type",
    ),
    e("coalesce_struct", Passthrough, Misc, "Spark-native"),
    e(
        "li_groot_cast_nullability",
        Disposition::UnsupportedBySpark,
        Misc,
        "LinkedIn-internal UDF",
    ),
    // ============================================================
    // Stage I additions — Hive built-ins that were in
    // StaticHiveFunctionRegistry but missed by the initial port.
    // These are all Spark-native or rewritten by FunctionRewriter.
    // ============================================================

    // Control-flow / null-handling keywords registered as functions in Hive.
    e(
        "case",
        Passthrough,
        Conditional,
        "Spark-native (CASE expression is a keyword, not a fn)",
    ),
    e(
        "when",
        Passthrough,
        Conditional,
        "Spark-native (WHEN branch of CASE)",
    ),
    e(
        "in",
        Passthrough,
        Conditional,
        "Spark-native (IN predicate)",
    ),
    e(
        "between",
        Passthrough,
        Conditional,
        "Spark-native (BETWEEN predicate)",
    ),
    e(
        "nullif",
        Passthrough,
        Null,
        "Spark-native: NULLIF(x, y) → NULL if x=y else x",
    ),
    e("isnull", Passthrough, Null, "Spark-native"),
    e("isnotnull", Passthrough, Null, "Spark-native"),
    // Hive emits these as internal tokens in some versions; treat same as
    // IS NULL / IS NOT NULL pass-throughs.
    e(
        "tok_isnull",
        Passthrough,
        Null,
        "Hive internal token, Spark-compatible",
    ),
    e(
        "tok_isnotnull",
        Passthrough,
        Null,
        "Hive internal token, Spark-compatible",
    ),
    // Window-function missing names.
    e("cume_dist", Passthrough, Window, "Spark-native"),
    e("percent_rank", Passthrough, Window, "Spark-native"),
    e("first_value", Passthrough, Window, "Spark-native"),
    e("last_value", Passthrough, Window, "Spark-native"),
    e("nth_value", Passthrough, Window, "Spark-native"),
    e("lag", Passthrough, Window, "Spark-native"),
    e("lead", Passthrough, Window, "Spark-native"),
    // Variance / stddev family.
    e(
        "variance",
        Passthrough,
        Aggregate,
        "Spark-native alias of var_samp",
    ),
    e("var_pop", Passthrough, Aggregate, "Spark-native"),
    e("var_samp", Passthrough, Aggregate, "Spark-native"),
    e(
        "stddev",
        Passthrough,
        Aggregate,
        "Spark-native alias of stddev_samp",
    ),
    e("stddev_pop", Passthrough, Aggregate, "Spark-native"),
    e("stddev_samp", Passthrough, Aggregate, "Spark-native"),
    // String / regex built-ins that were missing.
    e("replace", Passthrough, String, "Spark-native"),
    e(
        "translate3",
        Passthrough,
        String,
        "Hive 3-arg TRANSLATE, Spark-compatible",
    ),
    e("rlike", Passthrough, String, "Spark-native operator form"),
    e("regexp", Passthrough, String, "Spark-native alias of rlike"),
    // Date / time conversions.
    e(
        "timestamp_from_unixtime",
        Disposition::Rename("TIMESTAMP_SECONDS"),
        DateTime,
        "Hive → Spark rename",
    ),
    e(
        "from_utf8",
        Disposition::Rename("DECODE"),
        String,
        "Trino-compat name; Spark has DECODE(bytes, 'UTF-8')",
    ),
    // Hive reflection / Java method dispatch. Spark SQL doesn't have a
    // direct equivalent; emit with a warning so users see it.
    e(
        "reflect",
        Disposition::UnsupportedBySpark,
        Misc,
        "Hive reflect(Java method) — no Spark equivalent",
    ),
    e(
        "java_method",
        Disposition::UnsupportedBySpark,
        Misc,
        "alias of reflect",
    ),
    // Schema-handling helper used inside Coral itself.
    e(
        "generic_project",
        Disposition::UnsupportedBySpark,
        Misc,
        "Hive schema-projection UDTF; Coral-internal",
    ),
    // ============================================================
    // LinkedIn / Dali / Orbit FQN UDFs from the Java registry.
    //
    // These are kept as UnsupportedBySpark passthroughs so the
    // translator doesn't rewrite them and so `--list-functions`
    // reports them as known entities (avoids false-positive "unknown
    // function" warnings when users feed real LinkedIn production
    // views through coral-rust).
    //
    // Total ~100 fully-qualified names from the Java registry.
    // ============================================================
    e(
        "com.linkedin.dali.bug.dummyudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali bug test UDF",
    ),
    e(
        "com.linkedin.dali.customudf.date.hive.dateformattoepoch",
        Disposition::UnsupportedBySpark,
        DateTime,
        "Dali UDF",
    ),
    e(
        "com.linkedin.dali.udf.date.hive.dateformattoepoch",
        Disposition::UnsupportedBySpark,
        DateTime,
        "Dali UDF",
    ),
    e(
        "com.linkedin.dali.udf.date.hive.epochtodateformat",
        Disposition::UnsupportedBySpark,
        DateTime,
        "Dali UDF",
    ),
    e(
        "com.linkedin.dali.udf.date.hive.epochtoepochmilliseconds",
        Disposition::UnsupportedBySpark,
        DateTime,
        "Dali UDF",
    ),
    e(
        "com.linkedin.dali.udf.genericlookup.hive.genericlookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali lookup UDF",
    ),
    e(
        "com.linkedin.dali.udf.isguestmemberid.hive.isguestmemberid",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali UDF",
    ),
    e(
        "com.linkedin.dali.udf.maplookup.hive.maplookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali lookup UDF",
    ),
    e(
        "com.linkedin.dali.udf.monarch.urngenerator",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali URN UDF",
    ),
    e(
        "com.linkedin.dali.udf.portallookup.hive.portallookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali lookup UDF",
    ),
    e(
        "com.linkedin.dali.udf.sanitize.hive.sanitize",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali UDF",
    ),
    e(
        "com.linkedin.dali.udf.urnextractor.hive.urnextractor",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali URN UDF",
    ),
    e(
        "com.linkedin.dali.udf.useragentparser.hive.useragentparser",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali UA UDF",
    ),
    e(
        "com.linkedin.dali.udf.userinterfacelookup.hive.userinterfacelookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali lookup UDF",
    ),
    e(
        "com.linkedin.dali.udf.watbotcrawlerlookup.hive.watbotcrawlerlookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali lookup UDF",
    ),
    e(
        "com.linkedin.dali.view.udf.entityhandles.epochtimeinseconds",
        Disposition::UnsupportedBySpark,
        DateTime,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.view.udf.entityhandles.epochtimeinsecondsnullable",
        Disposition::UnsupportedBySpark,
        DateTime,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.view.udf.entityhandles.getidfromurn",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.view.udf.entityhandles.getpermissionsstring",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.view.udf.entityhandles.isurnfortype",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.view.udf.entityhandles.phonenumbernormalizer",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.views.job.udf.getuuid",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.views.premium.udf.getchooserid",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.views.premium.udf.getfamily",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.views.premium.udf.getorderurn",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.views.premium.udf.getpriceurnlist",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali view UDF",
    ),
    e(
        "com.linkedin.dali.views.search.udf.createsearchactionresultudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali search UDF",
    ),
    e(
        "com.linkedin.dali.views.search.udf.getactiontypeudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali search UDF",
    ),
    e(
        "com.linkedin.dali.views.search.udf.gettyahresulttypeudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali search UDF",
    ),
    e(
        "com.linkedin.dali.views.search.udf.getverticaludf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali search UDF",
    ),
    e(
        "com.linkedin.dali.views.search.udf.istyahsearchresultsudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali search UDF",
    ),
    e(
        "com.linkedin.dali.views.search.udf.isvalidkeyudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Dali search UDF",
    ),
    e(
        "com.linkedin.ds.udf.hive.filter.istestmemberid",
        Disposition::UnsupportedBySpark,
        Misc,
        "DS filter UDF",
    ),
    e(
        "com.linkedin.ds.udf.hive.lookup.portallookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "DS lookup UDF",
    ),
    e(
        "com.linkedin.ds.udf.hive.lookup.userinterfacelookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "DS lookup UDF",
    ),
    e(
        "com.linkedin.ds.udf.hive.lookup.watbotcrawlerlookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "DS lookup UDF",
    ),
    e(
        "com.linkedin.dwh.udf.hive.lookup.oslookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "DWH lookup UDF",
    ),
    e(
        "com.linkedin.dwh.udf.profile.getprofileurl",
        Disposition::UnsupportedBySpark,
        Misc,
        "DWH profile UDF",
    ),
    e(
        "com.linkedin.dwh.udf.sessionization.cleanupbrowserid",
        Disposition::UnsupportedBySpark,
        Misc,
        "DWH sessionization UDF",
    ),
    e(
        "com.linkedin.etg.business.common.udfs.mapd365optionset",
        Disposition::UnsupportedBySpark,
        Misc,
        "ETG UDF",
    ),
    e(
        "com.linkedin.etg.business.common.udfs.mapsfdcproductcode",
        Disposition::UnsupportedBySpark,
        Misc,
        "ETG UDF",
    ),
    e(
        "com.linkedin.etg.business.common.udfs.mapsfdcproductid",
        Disposition::UnsupportedBySpark,
        Misc,
        "ETG UDF",
    ),
    e(
        "com.linkedin.etg.business.common.udfs.mapsfdcproductname",
        Disposition::UnsupportedBySpark,
        Misc,
        "ETG UDF",
    ),
    e(
        "com.linkedin.groot.runtime.udf.spark.extractcollectionudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Groot UDF",
    ),
    e(
        "com.linkedin.groot.runtime.udf.spark.getmappedvalueudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Groot UDF",
    ),
    e(
        "com.linkedin.groot.runtime.udf.spark.hasmemberconsentudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Groot UDF",
    ),
    e(
        "com.linkedin.groot.runtime.udf.spark.redactfieldifudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Groot UDF",
    ),
    e(
        "com.linkedin.groot.runtime.udf.spark.redactsecondaryschemafieldifudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Groot UDF",
    ),
    e(
        "com.linkedin.jemslookup.udf.hive.jemslookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "Jems lookup UDF",
    ),
    e(
        "com.linkedin.jobs.udf.hive.convertindustrycode",
        Disposition::UnsupportedBySpark,
        Misc,
        "Jobs UDF",
    ),
    e(
        "com.linkedin.orbit.emerger.coercerudfs.dynamicslineofbusinesscoercer",
        Disposition::UnsupportedBySpark,
        Misc,
        "Orbit UDF",
    ),
    e(
        "com.linkedin.orbit.emerger.coercerudfs.generateid",
        Disposition::UnsupportedBySpark,
        Misc,
        "Orbit UDF",
    ),
    e(
        "com.linkedin.policy.decoration.udfs.hasmemberconsent",
        Disposition::UnsupportedBySpark,
        Misc,
        "Policy UDF",
    ),
    e(
        "com.linkedin.policy.decoration.udfs.redactfieldif",
        Disposition::UnsupportedBySpark,
        Misc,
        "Policy UDF",
    ),
    e(
        "com.linkedin.policy.decoration.udfs.redactsecondaryschemafieldif",
        Disposition::UnsupportedBySpark,
        Misc,
        "Policy UDF",
    ),
    e(
        "com.linkedin.recruiter.udf.geteventoriginudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Recruiter UDF",
    ),
    e(
        "com.linkedin.recruiter.udf.queryroutingtypeudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Recruiter UDF",
    ),
    e(
        "com.linkedin.snapshot.udf.constructsnapshoturnudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Snapshot UDF",
    ),
    e(
        "com.linkedin.snapshot.udf.snapshotpurgeeligibleudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "Snapshot UDF",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.dateformattoepoch",
        Disposition::UnsupportedBySpark,
        DateTime,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.epochtodateformat",
        Disposition::UnsupportedBySpark,
        DateTime,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.epochtoepochmilliseconds",
        Disposition::UnsupportedBySpark,
        DateTime,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.isguestmemberid",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.istestmemberid",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.maplookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.portallookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.sanitize",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.daliudfs.hive.watbotcrawlerlookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs alias",
    ),
    e(
        "com.linkedin.stdudfs.hive.daliudfs.urnextractorfunctionwrapper",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs wrapper",
    ),
    e(
        "com.linkedin.stdudfs.lookup.hive.browserlookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs lookup",
    ),
    e(
        "com.linkedin.stdudfs.parsing.hive.ip2str",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs parser",
    ),
    e(
        "com.linkedin.stdudfs.parsing.hive.useragentparser",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs parser",
    ),
    e(
        "com.linkedin.stdudfs.stringudfs.hive.initcap",
        Disposition::UnsupportedBySpark,
        String,
        "StdUDFs string",
    ),
    e(
        "com.linkedin.stdudfs.urnextractor.hive.urnextractorfunctionwrapper",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs wrapper",
    ),
    e(
        "com.linkedin.stdudfs.userinterfacelookup.hive.userinterfacelookup",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs lookup",
    ),
    e(
        "com.linkedin.stdudfs.userinterfacelookuptest.hive.userinterfacelookuptest",
        Disposition::UnsupportedBySpark,
        Misc,
        "StdUDFs test lookup",
    ),
    e(
        "com.linkedin.tsar.hive.udf.tojymbiiscores",
        Disposition::UnsupportedBySpark,
        Misc,
        "TSAR UDF",
    ),
    e(
        "com.linkedin.tscp.reporting.dali.udfs.activityid",
        Disposition::UnsupportedBySpark,
        Misc,
        "TSCP UDF",
    ),
    e(
        "com.linkedin.tscp.reporting.dali.udfs.adclickclassifier",
        Disposition::UnsupportedBySpark,
        Misc,
        "TSCP UDF",
    ),
    e(
        "com.linkedin.tscp.reporting.dali.udfs.adplacementclassifier",
        Disposition::UnsupportedBySpark,
        Misc,
        "TSCP UDF",
    ),
    e(
        "com.linkedin.tscp.reporting.dali.udfs.sponsoredmessagenodeid",
        Disposition::UnsupportedBySpark,
        Misc,
        "TSCP UDF",
    ),
    e(
        "com.linkedin.tscp.reporting.dali.udfs.unifiedcampaigntype",
        Disposition::UnsupportedBySpark,
        Misc,
        "TSCP UDF",
    ),
    e(
        "com.linkedin.tscp.reporting.dali.udfs.urntoid",
        Disposition::UnsupportedBySpark,
        Misc,
        "TSCP UDF",
    ),
    e(
        "com.linkedin.udf.aws.readjsonudf",
        Disposition::UnsupportedBySpark,
        Json,
        "AWS JSON UDF",
    ),
    e(
        "com.linkedin.udf.hdfs.getdatasetnamefrompathudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "HDFS UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscateall",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatearray",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatearrayevolve",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatemap",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatemapevolve",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatemapkeyevolve",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatemapvalevolve",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatememberidnumeric",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatememberidnumericint",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatememberidnumericlong",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.udfs.standard.hive.obfuscatestruct",
        Disposition::UnsupportedBySpark,
        Misc,
        "Obfuscation UDF",
    ),
    e(
        "com.linkedin.vector.daliview.udf.presentdatatype",
        Disposition::UnsupportedBySpark,
        Misc,
        "Vector Dali UDF",
    ),
    e(
        "com.linkedin.vector.daliview.udf.presentmediatype",
        Disposition::UnsupportedBySpark,
        Misc,
        "Vector Dali UDF",
    ),
    e(
        "com.linkedin.vector.daliview.udf.unifyvideooraudiodurationmicroseconds",
        Disposition::UnsupportedBySpark,
        Misc,
        "Vector Dali UDF",
    ),
    e(
        "isb.getprofilesections",
        Disposition::UnsupportedBySpark,
        Misc,
        "ISB UDF",
    ),
    e(
        "org.apache.hadoop.hive.ql.udf.generic.genericproject",
        Disposition::UnsupportedBySpark,
        Misc,
        "Hive generic_project FQN alias",
    ),
    e(
        "udfs.seoreferrertrkudf",
        Disposition::UnsupportedBySpark,
        Misc,
        "SEO UDF",
    ),
];

// Re-exports for ergonomics inside the registry definition.
use Category::*;
use Disposition::Passthrough;

/// Shorthand for constructing a `FunctionEntry`. Keeps the static table above
/// readable — without this each entry would be ~4 lines.
const fn e(
    name: &'static str,
    disposition: Disposition,
    category: Category,
    notes: &'static str,
) -> FunctionEntry {
    FunctionEntry {
        lower_name: name,
        disposition,
        category,
        notes,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_many_entries() {
        // Sanity check: we promised ~160+ rules (135 Hive + ~30 GaussDB).
        let n = registry().len();
        assert!(n > 150, "registry size = {n}");
    }

    #[test]
    fn case_insensitive_lookup() {
        assert!(lookup("NVL").is_some());
        assert!(lookup("nvl").is_some());
        assert!(lookup("Nvl").is_some());
    }

    #[test]
    fn all_stage1_rewrites_are_registered() {
        for (name, want) in [
            ("nvl", "COALESCE"),
            ("array_agg", "COLLECT_LIST"),
            ("random", "RAND"),
            ("substr", "SUBSTRING"),
            ("generate_series", "SEQUENCE"),
            ("bool_and", "EVERY"),
            ("bool_or", "SOME"),
        ] {
            let e = lookup(name).unwrap_or_else(|| panic!("missing: {name}"));
            match e.disposition {
                Disposition::Rename(new) => assert_eq!(new, want, "{name}"),
                other => panic!("{name}: expected Rename({want:?}), got {other:?}"),
            }
        }
    }

    #[test]
    fn passthrough_functions_are_passthrough() {
        for name in ["count", "sum", "concat", "cos", "sqrt", "date_add"] {
            assert_eq!(
                lookup(name).unwrap().disposition,
                Disposition::Passthrough,
                "{name} should be passthrough"
            );
        }
    }

    #[test]
    fn unknown_functions_return_none() {
        assert!(lookup("frobnicate_the_baz").is_none());
        assert!(!is_known("nonesuch"));
    }

    #[test]
    fn category_counts_are_sane() {
        use std::collections::HashMap as Hm;
        let mut by_cat: Hm<Category, usize> = Hm::new();
        for e in registry().values() {
            *by_cat.entry(e.category).or_default() += 1;
        }
        // String functions are the biggest category, more than 20 entries.
        assert!(
            by_cat.get(&Category::String).copied().unwrap_or(0) > 20,
            "String category count too small: {by_cat:?}"
        );
    }
}
