# coral-viz

SQL AST visualization — Rust port of Java `coral-visualization`. Produces
Graphviz DOT or PlantUML source text from any SQL string coral-core can
parse.

## Install

```toml
[dependencies]
coral-viz = "0.1"
```

## Use

```rust
use coral_viz::{render, Format};

let dot = render(
    "WITH x AS (SELECT a FROM t WHERE id > 1)
     SELECT a, COUNT(*) FROM x GROUP BY a HAVING COUNT(*) > 3",
    Format::Dot,
).unwrap();

println!("{dot}");
// Pipe to graphviz:
//   cargo run --example render > /tmp/q.dot && dot -Tsvg /tmp/q.dot -o /tmp/q.svg
```

Or PlantUML:

```rust
let puml = coral_viz::render("SELECT 1", coral_viz::Format::PlantUml).unwrap();
// plantuml -tsvg /tmp/q.puml
```

## Output layout

Each statement becomes a tree rooted at a `Query` / `CREATE VIEW` / etc.
node, with semantic children:

```
Query
├── WITH (1 CTE)
│   └── CTE x
│       └── Query
│           └── Select
│               ├── FROM (1 source)
│               │   └── t
│               ├── SELECT (1 col)
│               │   └── a
│               └── WHERE
│                   └── >
│                       ├── id
│                       └── 1
└── Select
    ├── FROM (1 source)
    │   └── x
    ├── SELECT (2 cols)
    │   ├── a
    │   └── COUNT()
    │       └── *
    ├── GROUP BY
    │   └── a
    └── HAVING
        └── >
            ├── COUNT()
            └── 3
```

Node kinds are color-coded:

| Kind | Color |
|---|---|
| Statement / Query / Subquery / CTE | `#E8EAF6` |
| Select / Projection | `#E3F2FD` |
| From / Table / Join | `#FFF3E0` |
| Filter / Having | `#FCE4EC` |
| GroupBy / OrderBy / Limit | `#F1F8E9` |
| SetOp (UNION / INTERSECT) | `#EDE7F6` |
| Expression | `#FFFFFF` |
| Values / Misc | `#EEEEEE` |

## Embedding in coral-service

The HTTP service at `POST /api/visualizations/generategraphs` uses this
crate directly. The payload accepts `format: "dot"` (default) or
`format: "plantuml"`.

## Tests

```bash
cargo test -p coral-viz
# 13 tests: DOT + PlantUML smoke + walker tree shape + error paths.
```
