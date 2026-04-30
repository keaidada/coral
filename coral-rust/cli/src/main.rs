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
//! echo "SELECT NVL(x, 0) FROM t" | coral
//! echo "SELECT RAND() FROM t"    | coral --target trino
//! coral --file query.sql --target trino
//! coral --smoke                  # 6 samples, Spark output (default)
//! coral --smoke --target trino   # same 6 samples, Trino output
//! ```

use std::io::Read;

use anyhow::{bail, Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "coral",
    version,
    about = "GaussDB / Hive → Spark or Trino SQL translator"
)]
struct Cli {
    /// Path to a SQL file; when omitted, reads from stdin.
    #[arg(long, short)]
    file: Option<std::path::PathBuf>,

    /// Run the built-in demo (the 6 samples from coral-gaussdb-spark's
    /// SmokeDemo). Honors `--target`.
    #[arg(long)]
    smoke: bool,

    /// Print the function coverage table (what Spark/Trino name each
    /// Hive/GaussDB function maps to).
    #[arg(long)]
    list_functions: bool,

    /// Output dialect: `spark` (default) or `trino`.
    #[arg(long, default_value = "spark")]
    target: String,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    let target = coral_core::Target::parse(&cli.target)
        .with_context(|| format!("unknown --target {:?} (use 'spark' or 'trino')", cli.target))?;

    if cli.smoke {
        return run_smoke(target);
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

    let out = coral_core::translate_to(&input, target)
        .with_context(|| format!("translating GaussDB → {target}"))?;
    println!("{}", out);
    Ok(())
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

fn run_smoke(target: coral_core::Target) -> Result<()> {
    println!("# target: {target}\n");
    let mut any_error = false;
    for (i, sql) in SMOKE_SAMPLES.iter().enumerate() {
        println!("\n=============== sample #{} ===============", i + 1);
        println!("[GaussDB]\n  {}\n", sql.trim().replace('\n', "\n  "));
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
