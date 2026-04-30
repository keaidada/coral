#!/usr/bin/env python3
# Copyright 2026 coral-rust contributors
# Licensed under the BSD-2-Clause license.
"""
Smoke demo for the Python binding. Mirrors the 6 samples from Java's
coral-gaussdb-spark SmokeDemo so Python users can directly compare output.

Run:

    cd coral-rust
    cargo build -p coral-ffi --release
    python3 ffi/examples/python/smoke_demo.py
"""

from __future__ import annotations

import sys
import textwrap

# Make the sibling coral.py importable.
import pathlib
sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import coral


SAMPLES = [
    # 1) CTE + JOIN + window + NVL + ||
    textwrap.dedent("""
        WITH active_emp AS (
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
        ORDER BY SUM(e.salary) DESC
    """).strip(),
    # 2) :: + DECODE + SUBSTR + MOD
    textwrap.dedent("""
        SELECT
            id::BIGINT AS id64,
            DECODE(dept_id, 1, 'eng', 2, 'sales', 'other') AS dept_label,
            SUBSTR(name, 1, 3) AS short_name,
            MOD(id, 10) AS bucket
        FROM employees WHERE dept_id IN (1, 2, 3)
    """).strip(),
    # 3) Regex + UNION
    "SELECT id FROM employees WHERE name ~* '^a.*' "
    "UNION ALL "
    "SELECT id FROM employees WHERE dept_id IN (SELECT id FROM departments WHERE name ~ 'Eng')",
    # 4) MERGE INTO
    textwrap.dedent("""
        MERGE INTO employees t USING departments s ON t.dept_id = s.id
          WHEN MATCHED THEN UPDATE SET name = s.name
          WHEN NOT MATCHED THEN INSERT (id, name) VALUES (s.id, s.name)
    """).strip(),
    # 5) CONNECT BY
    "SELECT id, name FROM employees "
    "START WITH mgr_id IS NULL "
    "CONNECT BY PRIOR id = mgr_id",
    # 6) DISTINCT ON
    "SELECT DISTINCT ON (dept_id) id, dept_id, salary "
    "FROM employees ORDER BY dept_id, salary DESC",
]


def main() -> int:
    print(f"coral-ffi v{coral.version()}\n")
    errors = 0
    for i, sql in enumerate(SAMPLES, start=1):
        print(f"========= sample #{i} =========")
        print(f"[GaussDB]\n{sql}\n")
        try:
            spark = coral.translate(sql)
            print(f"[Spark]\n{spark}\n")
        except coral.CoralError as e:
            print(f"[ERROR] {e}\n")
            errors += 1
    print("========= done =========")
    return 0 if errors == 0 else 1


if __name__ == "__main__":
    sys.exit(main())
