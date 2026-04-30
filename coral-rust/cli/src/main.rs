// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! coral — GaussDB / openGauss SQL -> Spark SQL translator.
//!
//! Rust port of `coral-gaussdb-spark`'s `SmokeDemo` / `CoralGaussDBToSpark` CLI
//! surface. Reads SQL from stdin (or `--file`) and prints Spark SQL to stdout.
//!
//! ```bash
//! echo "SELECT NVL(x, 0) FROM t" | coral
//! coral --file query.sql
//! coral --smoke     # run the same 6 samples coral-gaussdb-spark's SmokeDemo does
//! ```

use std::io::Read;

use anyhow::{Context, Result};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(name = "coral", version, about = "GaussDB -> Spark SQL translator")]
struct Cli {
    /// Path to a SQL file; when omitted, reads from stdin.
    #[arg(long, short)]
    file: Option<std::path::PathBuf>,

    /// Run the built-in demo (the 6 samples from coral-gaussdb-spark SmokeDemo).
    #[arg(long)]
    smoke: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    if cli.smoke {
        return run_smoke();
    }

    let input = match cli.file {
        Some(path) => std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?,
        None => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("reading stdin")?;
            buf
        }
    };

    let spark = coral_core::translate(&input).context("translating GaussDB -> Spark")?;
    println!("{}", spark);
    Ok(())
}

fn run_smoke() -> Result<()> {
    for (i, sql) in SMOKE_SAMPLES.iter().enumerate() {
        println!("\n=============== sample #{} ===============", i + 1);
        println!("[GaussDB]\n  {}\n", sql.trim().replace('\n', "\n  "));
        match coral_core::translate(sql) {
            Ok(spark) => println!("[Spark]\n  {}", spark.replace('\n', "\n  ")),
            Err(e) => println!("[ERROR] {}", e),
        }
    }
    println!("\n=============== done ===============");
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
