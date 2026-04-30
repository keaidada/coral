# coral-service

HTTP service wrapping [coral-core](../core) + [coral-trino](../trino). Rust
port of Java `coral-service` (Spring Boot → axum).

## Run

```bash
# From the repo root:
cargo run --release -p coral-service
# → coral-service listening on 0.0.0.0:8080

# Custom bind:
CORAL_BIND=127.0.0.1:9000 cargo run --release -p coral-service

# Debug logs:
RUST_LOG=coral_service=debug cargo run -p coral-service
```

Cold start on a 2023 MBP: **~45 ms** (measured with `hyperfine`), vs the
Java service's ~3.5 s with Spring Boot autoconfiguration.

## Endpoints

All JSON unless noted. Request/response field names match Java
`coral-service`'s camelCase schema, so existing clients work verbatim.

### `POST /api/translations/translate`

Translate SQL.

```bash
curl -s http://localhost:8080/api/translations/translate \
  -H 'content-type: application/json' \
  -d '{"query": "SELECT NVL(a, 0) FROM t", "targetLanguage": "spark"}'

# {"translated":"SELECT COALESCE(a, 0) FROM t","target":"spark","issues":[]}
```

- `query` (string, required) — source SQL
- `sourceLanguage` (string, optional) — ignored today; accepted for
  client compatibility. All of Hive / Spark / GaussDB / Trino input
  parse through the same PostgreSQL dialect.
- `targetLanguage` (string, optional, default `spark`) — `spark`,
  `trino`, or `presto`
- `rewriteType` (string, optional, reserved) — `INCREMENTAL` |
  `DATAMASKING` | `NONE`

Response: `{translated, target, issues[], error?}`. Parse / translate
errors come back as 200-with-`error` (matches Java behavior so that
browser clients can treat them as data).

### `POST /api/translations/validate`

Parse-only check.

```bash
curl -s http://localhost:8080/api/translations/validate \
  -H 'content-type: application/json' \
  -d '{"query": "SELECT 1; SELECT 2"}'

# {"parses":true,"statementCount":2}
```

### `POST /api/visualizations/generategraphs`

Build a DOT or PlantUML graph of the parsed AST, cache it, return an ID.

```bash
curl -s http://localhost:8080/api/visualizations/generategraphs \
  -H 'content-type: application/json' \
  -d '{"query": "SELECT 1; SELECT 2", "format": "dot"}'

# {"graphId":"17a1b2c3d4e5f678","format":"dot"}
```

- `format` (string, optional, default `dot`) — `dot` | `plantuml`

### `GET /api/visualizations/{id}`

Fetch a previously generated graph source. Content-Type is
`text/vnd.graphviz` for DOT, `text/plain` for PlantUML. You render
client-side (e.g. `dot -Tsvg` or `plantuml -tsvg`).

Graphs live in-memory, so they vanish on restart — the Java service
has the same semantics.

### `GET /api/functions`

Dump the full function registry as JSON.

```bash
curl -s http://localhost:8080/api/functions | jq '.total, .entries[0]'

# 198
# {"name":"abs","disposition":"passthrough","category":"math","notes":"..."}
```

### `GET /api/health`

Liveness probe.

```bash
curl -s http://localhost:8080/api/health
# {"status":"ok","version":"0.1.0"}
```

## Tests

```bash
cargo test -p coral-service
# 12 tests wired via axum's `oneshot` — no TCP port bound during tests.
```

## See also

- [coral-core](../core) — translation pipeline
- [coral-trino](../trino) — Trino output backend
