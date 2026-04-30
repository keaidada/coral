SELECT NVL(NVL(DECODE(x, 1, 'a', 2, 'b', 'c'), MOD(id, 5)::STRING), 'fallback') FROM t
