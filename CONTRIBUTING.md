# Contributing

This branch is Rust-first. The legacy Java / Gradle backend has been removed.

Before submitting changes, run the relevant checks:

## Rust

```bash
cd coral-rust
cargo fmt --all
cargo clippy --all-targets --workspace -- -D warnings
cargo test --workspace --all-targets
```

## Frontend

```bash
cd coral-service/frontend
npm run lint
npm run build
```

Bug fixes should include a regression test when practical. Feature changes should update the relevant README or module documentation.
