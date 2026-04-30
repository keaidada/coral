// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! Cross-crate integration-test harness for the coral-rust workspace.
//!
//! No code here — tests live under `tests/`. The crate is `publish =
//! false` and exists solely to pull every workspace crate in as a
//! dev-dependency so we can exercise them in a single binary.

// keep clippy happy about the empty crate.
#[doc(hidden)]
pub fn _ping() {}
