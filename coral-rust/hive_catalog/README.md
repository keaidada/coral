# coral-hive-catalog

Hive Metastore / Iceberg REST / Unity Catalog client for [`coral-core`](https://crates.io/crates/coral-core). Implements the `Catalog` trait so `translate_with_catalog` can validate queries against a live schema store.

## Quickstart

```rust
use coral_hive_catalog::{HiveCatalog, HttpTransport};

let transport = HttpTransport::new("http://metastore:9083/api");
let catalog = HiveCatalog::new(transport);

let r = coral_core::translate_with_catalog(
    "SELECT e.id FROM analytics.employees e",
    &catalog,
)?;
// r.spark_sql   — translated SQL
// r.issues      — UnknownTable / UnknownColumn warnings (with did-you-mean)
```

## Transport is pluggable

The default `HttpTransport` uses blocking `ureq`. If your runtime is async (tokio / async-std), implement the `Transport` trait against `reqwest`:

```rust
impl Transport for MyReqwestTransport {
    fn get_table_schema(&self, db: &str, table: &str)
        -> Result<Option<TableSchemaJson>, CatalogError> { ... }
}
```

The `Transport` trait is `Send + Sync` so a single catalog can be shared across threads.

## Caching

Column lookups are cached in-process after the first successful fetch. Negative responses (404 / not found) are cached too so absent tables don't spam the server. Invalidate with `HiveCatalog::invalidate(db, table)` or `HiveCatalog::invalidate_all()`.

The cache hands out `&'static [String]` slices via `Box::leak` — see the crate-level docs for why. Net memory cost: ~32 bytes per unique table queried during the process lifetime.

## Wire format

The REST response is expected to look like:

```json
{
  "columns": [
    { "name": "id", "type": "bigint" },
    { "name": "name", "type": "string" }
  ]
}
```

`type_text` is accepted as an alias for `type` (Unity Catalog style).

## License

BSD-2-Clause
