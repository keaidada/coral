#!/usr/bin/env python3
# Copyright 2026 coral-rust contributors
# Licensed under the BSD-2-Clause license.
"""
Pre-build hook: copy the freshly compiled libcoral_ffi cdylib from the Cargo
target dir into src/coral_sql/_lib/ so it ships inside the wheel.

Invoke manually or from CI:

    cd coral-rust/python
    python build.py                    # uses ../target/release
    python build.py --debug            # uses ../target/debug

Then run the standard wheel build:

    python -m build --wheel

The wheel will be platform-tagged (e.g. macosx_14_0_arm64, manylinux2014_x86_64)
because it contains a native library.
"""

from __future__ import annotations

import argparse
import pathlib
import shutil
import sys


def lib_filename() -> str:
    if sys.platform == "win32":
        return "coral_ffi.dll"
    if sys.platform == "darwin":
        return "libcoral_ffi.dylib"
    return "libcoral_ffi.so"


def main() -> int:
    ap = argparse.ArgumentParser(description="Stage coral-ffi cdylib into the Python wheel")
    ap.add_argument("--debug", action="store_true", help="Use target/debug instead of target/release")
    args = ap.parse_args()

    here = pathlib.Path(__file__).resolve().parent
    workspace = here.parent  # coral-rust/

    src_dir = workspace / ("target/debug" if args.debug else "target/release")
    src = src_dir / lib_filename()
    if not src.exists():
        print(f"error: {src} not found.", file=sys.stderr)
        print("hint: run `cargo build -p coral-ffi --release` (or --debug) first.", file=sys.stderr)
        return 1

    dst_dir = here / "src" / "coral_sql" / "_lib"
    dst_dir.mkdir(parents=True, exist_ok=True)
    # Remove any stale libs from a previous platform.
    for old in dst_dir.glob("libcoral_ffi.*"):
        old.unlink()
    for old in dst_dir.glob("coral_ffi.*"):
        old.unlink()

    dst = dst_dir / src.name
    shutil.copy2(src, dst)
    print(f"copied {src}  ->  {dst}  ({src.stat().st_size:,} bytes)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
