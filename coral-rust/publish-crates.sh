#!/usr/bin/env bash
# Copyright 2026 coral-rust contributors
# Publish coral-core / coral-cli / coral-ffi to crates.io.
#
# Run in this exact order (crates.io requires deps to be resolvable):
#
#   1. cargo login <YOUR_CRATES_IO_TOKEN>
#   2. bash coral-rust/publish-crates.sh
#
# Each `cargo publish` triggers a server-side build + indexing, which
# takes ~30-60 s. We wait for coral-core to be queryable before attempting
# the dependent crates.

set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"

# ---- core first ----
echo "[1/3] Publishing coral-core ..."
cd "$here/core"
cargo publish "$@"
echo "      waiting for crates.io to index coral-core ..."
sleep 45

# ---- cli and ffi depend on coral-core; publish in parallel once core is live ----
echo "[2/3] Publishing coral-cli ..."
cd "$here/cli"
cargo publish "$@"

echo "[3/3] Publishing coral-ffi ..."
cd "$here/ffi"
cargo publish "$@"

echo "Done. Verify with: cargo search coral-core coral-cli coral-ffi"
