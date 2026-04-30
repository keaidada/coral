# Copyright 2026 coral-rust contributors
# Licensed under the BSD-2-Clause license.
"""coral-sql — GaussDB / openGauss SQL to Spark SQL translator.

Thin Python wrapper around the Rust `coral-ffi` cdylib. Uses only the
standard library (`ctypes`) at runtime — no PyO3, no Rust toolchain,
no Python build step once installed.

Typical use:

    from coral_sql import translate, CoralError

    try:
        spark = translate("SELECT NVL(x, 0), y::INT FROM t")
        print(spark)
    except CoralError as e:
        print("translation failed:", e)
"""

from __future__ import annotations

import ctypes
import os
import pathlib
import sys
from typing import Optional

__version__ = "0.1.0"


class CoralError(Exception):
    """Raised when the translator reports a parse or rewrite error."""


def _lib_filename() -> str:
    if sys.platform == "win32":
        return "coral_ffi.dll"
    if sys.platform == "darwin":
        return "libcoral_ffi.dylib"
    return "libcoral_ffi.so"


def _find_lib() -> str:
    filename = _lib_filename()

    # 1) Explicit override (set by CI or power users).
    override = os.environ.get("CORAL_FFI_LIB")
    if override and pathlib.Path(override).exists():
        return override

    # 2) Shipped inside the wheel at coral_sql/_lib/<filename>.
    here = pathlib.Path(__file__).resolve().parent
    bundled = here / "_lib" / filename
    if bundled.exists():
        return str(bundled)

    # 3) Dev-mode fallback: look in ../../../target/{release,debug} relative
    #    to this file (coral-rust/python/src/coral_sql/__init__.py).
    workspace = here.parent.parent.parent
    for subdir in ("target/release", "target/debug"):
        candidate = workspace / subdir / filename
        if candidate.exists():
            return str(candidate)

    raise FileNotFoundError(
        f"coral-sql: cannot find {filename}. "
        "Set CORAL_FFI_LIB=/path/to/libcoral_ffi.* or reinstall the wheel."
    )


_lib = ctypes.CDLL(_find_lib())

_lib.coral_translate.argtypes = [ctypes.c_char_p]
_lib.coral_translate.restype = ctypes.c_void_p
_lib.coral_free_string.argtypes = [ctypes.c_void_p]
_lib.coral_free_string.restype = None
_lib.coral_last_error.argtypes = []
_lib.coral_last_error.restype = ctypes.c_char_p
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
        return ctypes.c_char_p(raw).value.decode("utf-8")
    finally:
        _lib.coral_free_string(raw)


def ffi_version() -> str:
    """Return the underlying coral-ffi library version."""
    v = _lib.coral_version()
    return v.decode("utf-8") if v else ""


__all__ = ["translate", "ffi_version", "CoralError", "__version__"]
