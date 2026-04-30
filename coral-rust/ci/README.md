# coral-rust CI configuration

This directory holds the GitHub Actions workflow for coral-rust as a template. It lives here instead of `.github/workflows/` because the current OAuth token used to push to this repo lacks the `workflow` scope — so we ship the YAML as data, not as a live workflow, and let the maintainer enable it with one copy.

## Enable the workflow

```bash
# From the repo root, after cloning:
mkdir -p .github/workflows
cp coral-rust/ci/coral-rust.yml .github/workflows/
git add .github/workflows/coral-rust.yml
git commit -m "ci: enable coral-rust workflow"
git push
```

You'll need a push token with the `workflow` scope for that last push (e.g. a personal access token with `repo + workflow`, or GitHub Actions itself committing back).

## What the workflow does

Runs on every push / PR that touches `coral-rust/**`:

- **Matrix**: Linux, macOS, Windows
  - `cargo fmt --all -- --check`
  - `cargo clippy --all-targets --workspace -- -D warnings`
  - `cargo test --workspace --all-targets`
  - `cargo build --release --workspace`
  - `cargo run --release --bin coral -- --smoke` (CLI sanity)
  - Verify `libcoral_ffi.{so,dylib,dll}` exists
  - Run `ffi/examples/python/smoke_demo.py` (Python binding e2e)

- **Linux-only extras job**:
  - `cargo doc --workspace --no-deps -D warnings` (docs build clean)
  - MSRV check against Rust 1.75.0

Uses `Swatinem/rust-cache` for build-artifact caching and `dtolnay/rust-toolchain@stable` for a tight, fast setup. Concurrency group cancels stale runs when you force-push.
