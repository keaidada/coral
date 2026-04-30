SELECT id, name FROM employees START WITH mgr_id IS NULL CONNECT BY PRIOR id = mgr_id
