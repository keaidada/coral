SELECT id FROM employees WHERE name ~* '^a.*' UNION ALL SELECT id FROM employees WHERE dept_id IN (SELECT id FROM departments WHERE name ~ 'Eng')
