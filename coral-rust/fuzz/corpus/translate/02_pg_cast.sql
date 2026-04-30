SELECT id::BIGINT, DECODE(dept_id, 1, 'eng', 2, 'sales', 'other'), SUBSTR(name, 1, 3), MOD(id, 10) FROM employees
