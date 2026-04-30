// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! coral — GaussDB / openGauss SQL → Spark / Trino SQL translator.
//!
//! Rust port of `coral-gaussdb-spark`'s `SmokeDemo` /
//! `CoralGaussDBToSpark` + `coral-trino`'s `HiveToTrinoConverter` CLI
//! surfaces. Reads SQL from stdin (or `--file`) and prints the translated
//! SQL to stdout.
//!
//! ```bash
//! echo "SELECT NVL(x, 0) FROM t" | coral --source gaussdb --target spark
//! echo "SELECT RAND() FROM t"    | coral --source hive --target trino
//! coral --file query.sql --from trino --to spark
//! coral --smoke                  # 6 GaussDB samples, Spark output (default)
//! coral --smoke --target trino   # same 6 samples, Trino output
//! ```

use std::collections::BTreeSet;
use std::io::Read;

use anyhow::{bail, Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "coral",
    version,
    about = "本地 SQL 方言转换工具：GaussDB / Hive / Spark / Trino → Spark / Trino",
    override_usage = "coral [选项]",
    help_template = "{about-with-newline}\n用法: {usage}\n\n选项:\n{options}{after-help}",
    disable_help_flag = true,
    disable_version_flag = true,
    after_help = "\n示例:\n  coral --file query.sql --source gaussdb --target spark --pretty\n  cat hive.sql | coral --from hive --to trino\n  echo \"SELECT NVL(x, 0) FROM t\" | coral --source gaussdb --target spark"
)]
struct Cli {
    /// SQL 文件路径；不指定时从标准输入 stdin 读取。
    #[arg(long, short, value_name = "SQL文件")]
    file: Option<std::path::PathBuf>,

    /// 输入数据源/SQL 方言。可选值：gaussdb, opengauss, open_gauss, postgres, postgresql, hive, hiveql, spark, spark_sql, sparksql, trino, presto。默认：gaussdb。别名：--from。
    #[arg(
        long,
        alias = "from",
        default_value = "gaussdb",
        hide_default_value = true,
        value_name = "输入方言"
    )]
    source: String,

    /// 运行内置演示样例；会按照 --target 指定的目标方言输出。
    #[arg(long)]
    smoke: bool,

    /// 打印函数映射覆盖表，展示 Hive/GaussDB 函数如何映射到 Spark/Trino。
    #[arg(long)]
    list_functions: bool,

    /// 输出目标 SQL 方言。可选值：spark, trino, presto。默认：spark。别名：--to。
    #[arg(
        long,
        alias = "to",
        default_value = "spark",
        hide_default_value = true,
        value_name = "输出方言"
    )]
    target: String,

    /// 美化输出 SQL：表自动加别名，并将顶层子句换行；默认紧凑输出。
    #[arg(long)]
    pretty: bool,

    /// 允许未注册函数透传。可不带值放行全部未知函数，也可指定逗号分隔 UDF 白名单，例如：--allow-unknown-functions YNVL,XNVL。
    #[arg(
        long,
        value_name = "UDF列表",
        num_args = 0..=1,
        default_missing_value = "*"
    )]
    allow_unknown_functions: Option<String>,

    /// 显示帮助信息。
    #[arg(short = 'h', long = "help", action = clap::ArgAction::Help)]
    _help: Option<bool>,

    /// 显示版本信息。
    #[arg(short = 'V', long = "version", action = clap::ArgAction::Version)]
    _version: Option<bool>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let source = normalize_source(&cli.source)?;
    let target = coral_core::Target::parse(&cli.target)
        .with_context(|| format!("unknown --target {:?} (use 'spark' or 'trino')", cli.target))?;

    if cli.smoke {
        return run_smoke(&source, target);
    }

    if cli.list_functions {
        return print_function_catalog();
    }

    let input = match cli.file {
        Some(path) => {
            std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?
        }
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading stdin")?;
            buf
        }
    };

    let allow_unknown = parse_unknown_function_allowlist(cli.allow_unknown_functions.as_deref());
    let unknown = coral_core::unknown_functions(&input)?;
    let rejected = rejected_unknown_functions(&unknown, &allow_unknown);
    if !rejected.is_empty() {
        bail!(
            "发现未知函数: {}。如果这是业务 UDF，请使用 --allow-unknown-functions {}；也可不带值放行全部未知函数。",
            rejected.join(", "),
            rejected.join(",")
        );
    }

    let out = coral_core::translate_to_with(&input, target, cli.pretty)
        .with_context(|| format!("translating {source} → {target}"))?;
    println!("{}", out);
    Ok(())
}

fn normalize_source(source: &str) -> Result<String> {
    let lower = source.trim().to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "gaussdb" | "opengauss" | "open_gauss" | "postgres" | "postgresql" => "gaussdb",
        "hive" | "hiveql" => "hive",
        "spark" | "spark_sql" | "sparksql" => "spark",
        "trino" | "presto" => "trino",
        _ => bail!(
            "unknown --source {:?} (use 'gaussdb', 'hive', 'spark', or 'trino')",
            source
        ),
    };
    Ok(canonical.to_string())
}

fn source_label(source: &str) -> &'static str {
    match source {
        "gaussdb" => "GaussDB / openGauss",
        "hive" => "Hive",
        "spark" => "Spark",
        "trino" => "Trino",
        _ => "SQL",
    }
}

enum UnknownFunctionAllowlist {
    None,
    All,
    Names(BTreeSet<String>),
}

fn parse_unknown_function_allowlist(raw: Option<&str>) -> UnknownFunctionAllowlist {
    let Some(raw) = raw else {
        return UnknownFunctionAllowlist::None;
    };
    if raw.trim().is_empty() || raw.trim() == "*" {
        return UnknownFunctionAllowlist::All;
    }
    let names = raw
        .split(',')
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    UnknownFunctionAllowlist::Names(names)
}

fn rejected_unknown_functions(
    unknown: &[String],
    allowlist: &UnknownFunctionAllowlist,
) -> Vec<String> {
    match allowlist {
        UnknownFunctionAllowlist::All => vec![],
        UnknownFunctionAllowlist::None => unknown.to_vec(),
        UnknownFunctionAllowlist::Names(names) => unknown
            .iter()
            .filter(|name| !names.contains(name.as_str()))
            .cloned()
            .collect(),
    }
}

fn print_function_catalog() -> Result<()> {
    use coral_core::{Disposition, FunctionEntry};

    // Pull all entries, group by category for readability.
    let mut by_cat: std::collections::BTreeMap<&str, Vec<FunctionEntry>> =
        std::collections::BTreeMap::new();
    for entry in coral_core::function_catalog::registry().values() {
        by_cat
            .entry(format_category(entry.category))
            .or_default()
            .push(*entry);
    }

    println!(
        "coral-rust function coverage ({} entries)\n",
        coral_core::function_catalog::registry().len()
    );
    println!("{:<24} {:<14} {:<6} NOTES", "NAME", "DISPOSITION", "CAT");
    println!("{}", "-".repeat(90));

    for (cat_label, mut entries) in by_cat {
        entries.sort_by_key(|e| e.lower_name);
        for e in entries {
            let disp = match e.disposition {
                Disposition::Passthrough => "passthrough".to_string(),
                Disposition::Rename(n) => format!("=> {n}"),
                Disposition::CustomRewrite => "custom".to_string(),
                Disposition::UnsupportedBySpark => "UNSUPPORTED".to_string(),
            };
            println!(
                "{:<24} {:<14} {:<6} {}",
                e.lower_name, disp, cat_label, e.notes
            );
        }
    }
    Ok(())
}

fn format_category(c: coral_core::Category) -> &'static str {
    use coral_core::Category::*;
    match c {
        Aggregate => "agg",
        String => "str",
        Math => "math",
        DateTime => "date",
        Collection => "coll",
        Json => "json",
        Hash => "hash",
        Bitwise => "bit",
        Window => "win",
        Cast => "cast",
        Null => "null",
        Conditional => "cond",
        Xpath => "xpath",
        Udtf => "udtf",
        Context => "ctx",
        GaussDbSpecific => "gauss",
        Misc => "misc",
    }
}

fn run_smoke(source: &str, target: coral_core::Target) -> Result<()> {
    println!("# source: {}", source_label(source));
    println!("# target: {target}\n");
    let mut any_error = false;
    for (i, sql) in SMOKE_SAMPLES.iter().enumerate() {
        println!("\n=============== sample #{} ===============", i + 1);
        println!(
            "[{}]\n  {}\n",
            source_label(source),
            sql.trim().replace('\n', "\n  ")
        );
        match coral_core::translate_to(sql, target) {
            Ok(out) => println!("[{target}]\n  {}", out.replace('\n', "\n  ")),
            Err(e) => {
                println!("[ERROR] {}", e);
                any_error = true;
            }
        }
    }
    println!("\n=============== done ===============");
    if any_error {
        bail!("at least one sample failed to translate");
    }
    Ok(())
}

/// The 6 GaussDB samples from
/// `coral-gaussdb-spark/src/test/java/com/linkedin/coral/gaussdb/spark/SmokeDemo.java`.
/// Kept verbatim so output is directly comparable with the Java tree's output.
const SMOKE_SAMPLES: &[&str] = &[
    // 1) CTE + JOIN + GROUP BY + HAVING + window + CASE + NVL + ||
    "WITH active_emp AS (
        SELECT id, name, dept_id, salary, mgr_id FROM employees WHERE salary > 0
     )
     SELECT
        d.name || ' / ' || NVL(e.name, 'n/a') AS label,
        COUNT(*) AS headcount,
        SUM(e.salary) AS total_pay,
        CASE WHEN AVG(e.salary) > 100 THEN 'high' ELSE 'low' END AS tier,
        ROW_NUMBER() OVER (PARTITION BY d.id ORDER BY SUM(e.salary) DESC) AS rn
     FROM active_emp e
     LEFT JOIN departments d ON e.dept_id = d.id
     GROUP BY d.id, d.name, e.name
     HAVING COUNT(*) > 0
     ORDER BY SUM(e.salary) DESC",
    // 2) PG :: cast + DECODE + SUBSTR + MOD
    "SELECT
        id::BIGINT AS id64,
        DECODE(dept_id, 1, 'eng', 2, 'sales', 'other') AS dept_label,
        SUBSTR(name, 1, 3) AS short_name,
        MOD(id, 10) AS bucket
     FROM employees WHERE dept_id IN (1, 2, 3)",
    // 3) Regex + UNION + subquery
    "SELECT id FROM employees WHERE name ~* '^a.*'
     UNION ALL
     SELECT id FROM employees WHERE dept_id IN (SELECT id FROM departments WHERE name ~ 'Eng')",
    // 4) MERGE INTO
    "MERGE INTO employees t USING departments s ON t.dept_id = s.id
       WHEN MATCHED THEN UPDATE SET name = s.name
       WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)",
    // 5) CONNECT BY (recursive) — sqlparser-rs does NOT parse Oracle CONNECT BY
    //    as of 0.52; this sample exists to document that limitation.
    "SELECT id, name FROM employees
       START WITH mgr_id IS NULL CONNECT BY PRIOR id = mgr_id",
    // 6) DISTINCT ON
    "SELECT DISTINCT ON (dept_id) id, dept_id, salary
       FROM employees ORDER BY dept_id, salary DESC",
];
