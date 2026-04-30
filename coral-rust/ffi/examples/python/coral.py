# Copyright 2026 coral-rust contributors
# Licensed under the BSD-2-Clause license.
"""
Python binding for coral-ffi, using the standard-library `ctypes` module.

Usage:

    from coral import translate, CoralError

    try:
        spark_sql = translate("SELECT NVL(x, 0), y::INT FROM t")
        print(spark_sql)
    except CoralError as e:
        print("translation failed:", e)

The binding depends ONLY on ctypes (stdlib) and the libcoral_ffi shared
library built by `cargo build -p coral-ffi`. No need for PyO3, no need for
a Python build step, works with any CPython 3.8+.

On import, the module searches for libcoral_ffi in this order:
  1. $CORAL_FFI_LIB (an explicit path to the .so / .dylib / .dll)
  2. Alongside this file (same dir)
  3. ../target/release/ and ../target/debug/ (dev workflow)
"""

from __future__ import annotations

import ctypes
import os
import pathlib
import sys
from typing import Optional


class CoralError(Exception):
    """Raised when the translator reports a parse or rewrite error."""


def _find_lib() -> str:
    ext = {"linux": "so", "darwin": "dylib", "win32": "dll"}.get(sys.platform, "so")
    stem = "coral_ffi" if sys.platform == "win32" else "libcoral_ffi"
    filename = f"{stem}.{ext}"

    # 1) Explicit override.
    override = os.environ.get("CORAL_FFI_LIB")
    if override and pathlib.Path(override).exists():
        return override

    here = pathlib.Path(__file__).resolve().parent

    # 2) Sibling of this file (wheel / installed case).
    candidate = here / filename
    if candidate.exists():
        return str(candidate)

    # 3) Cargo target dirs relative to the coral-rust workspace.
    #    This file lives at coral-rust/ffi/examples/python/coral.py,
    #    so go up three levels to reach coral-rust/.
    workspace = here.parent.parent.parent
    for subdir in ("target/release", "target/debug"):
        candidate = workspace / subdir / filename
        if candidate.exists():
            return str(candidate)

    raise FileNotFoundError(
        f"Could not find {filename}. Set CORAL_FFI_LIB or run "
        "`cargo build -p coral-ffi` from the coral-rust/ workspace."
    )


_lib = ctypes.CDLL(_find_lib())

# void *coral_translate(const char *input);
_lib.coral_translate.argtypes = [ctypes.c_char_p]
_lib.coral_translate.restype = ctypes.c_void_p  # raw pointer so we can free it

# void coral_free_string(void *ptr);
_lib.coral_free_string.argtypes = [ctypes.c_void_p]
_lib.coral_free_string.restype = None

# const char *coral_last_error(void);
_lib.coral_last_error.argtypes = []
_lib.coral_last_error.restype = ctypes.c_char_p

# const char *coral_version(void);
_lib.coral_version.argtypes = []
_lib.coral_version.restype = ctypes.c_char_p


def translate(gaussdb_sql: str) -> str:
    """Translate a GaussDB / openGauss SQL statement to Spark SQL.

    Raises CoralError on parse or rewrite failure.
    """
    raw = _lib.coral_translate(gaussdb_sql.encode("utf-8"))
    if not raw:
        err = _lib.coral_last_error()
        msg = err.decode("utf-8", errors="replace") if err else "unknown error"
        raise CoralError(msg)
    try:
        # c_char_p with a raw address decodes the NUL-terminated string.
        return ctypes.c_char_p(raw).value.decode("utf-8")
    finally:
        _lib.coral_free_string(raw)


def version() -> str:
    """Return the coral-ffi library version."""
    v = _lib.coral_version()
    return v.decode("utf-8") if v else ""


__all__ = ["translate", "version", "CoralError"]
