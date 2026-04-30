// Copyright 2026 coral-rust contributors
// Licensed under the BSD-2-Clause license.

//! C ABI bindings for `coral-core`.
//!
//! All functions use plain C types (`const char*`, `char*`) so they can be
//! consumed from Python (via `ctypes`), Go (via cgo), Node (via `ffi-napi`),
//! Ruby, or plain C.
//!
//! # Memory model
//!
//! - **Inputs** (`const char*`) are borrowed: the caller retains ownership
//!   and guarantees NUL termination for the duration of the call.
//! - **Outputs** (`char*`) are owned by coral-ffi. The caller MUST pass the
//!   pointer back to [`coral_free_string`] when done. Freeing with any other
//!   allocator (including `libc::free`) is undefined behavior — the Rust
//!   `String` allocator may differ from the C `malloc` pool.
//!
//! # Example (C)
//!
//! ```c
//! #include "coral.h"
//!
//! char *spark = coral_translate("SELECT NVL(x, 0) FROM t");
//! if (spark) {
//!     puts(spark);         // "SELECT COALESCE(x, 0) FROM t"
//!     coral_free_string(spark);
//! } else {
//!     const char *err = coral_last_error();
//!     fprintf(stderr, "coral error: %s\n", err ? err : "(none)");
//! }
//! ```

use std::cell::RefCell;
use std::ffi::{c_char, CStr, CString};

thread_local! {
    /// Stores the most recent error message per thread. `coral_last_error()`
    /// reads from here. Cleared on every successful translation call.
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

fn set_error(msg: &str) {
    let c = CString::new(msg.replace('\0', " ")).unwrap_or_else(|_| CString::new("error").unwrap());
    LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(c));
}

fn clear_error() {
    LAST_ERROR.with(|cell| *cell.borrow_mut() = None);
}

/// Translate a NUL-terminated GaussDB/openGauss SQL string into Spark SQL.
///
/// Returns:
///   * A freshly-allocated `char*` with the Spark SQL on success. Caller MUST
///     free it with [`coral_free_string`].
///   * `NULL` on error. Call [`coral_last_error`] immediately after (same
///     thread) to retrieve the error message.
///
/// # Safety
///
/// `input` must be a valid NUL-terminated C string for the duration of this
/// call, and `input` must NOT be NULL. Passing a non-UTF-8 string is an error
/// (translation returns NULL with a "not UTF-8" message).
#[no_mangle]
pub unsafe extern "C" fn coral_translate(input: *const c_char) -> *mut c_char {
    clear_error();

    if input.is_null() {
        set_error("input pointer was NULL");
        return std::ptr::null_mut();
    }

    let c_input = unsafe { CStr::from_ptr(input) };
    let sql = match c_input.to_str() {
        Ok(s) => s,
        Err(e) => {
            set_error(&format!("input is not valid UTF-8: {e}"));
            return std::ptr::null_mut();
        }
    };

    match coral_core::translate(sql) {
        Ok(spark) => match CString::new(spark) {
            Ok(c) => c.into_raw(),
            Err(e) => {
                set_error(&format!("output contains interior NUL byte: {e}"));
                std::ptr::null_mut()
            }
        },
        Err(e) => {
            set_error(&e.to_string());
            std::ptr::null_mut()
        }
    }
}

/// Free a C string that was returned by [`coral_translate`]. Passing NULL is
/// a no-op. Passing a pointer that was NOT returned by this crate is
/// undefined behavior.
///
/// # Safety
///
/// `ptr` must have been returned by [`coral_translate`] (and not already
/// freed).
#[no_mangle]
pub unsafe extern "C" fn coral_free_string(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // Reconstruct the CString so its Drop impl frees with the Rust allocator.
    let _ = unsafe { CString::from_raw(ptr) };
}

/// Return a pointer to the most recent error message on this thread, or NULL
/// if the last call succeeded. The pointer is valid until the NEXT call to
/// `coral_translate` on the same thread — DO NOT free it, DO NOT cache across
/// calls.
///
/// # Safety
///
/// The returned pointer is not owned by the caller and may dangle after the
/// next `coral_translate` call on the same thread.
#[no_mangle]
pub extern "C" fn coral_last_error() -> *const c_char {
    LAST_ERROR.with(|cell| {
        cell.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |s| s.as_ptr())
    })
}

/// Library version as a NUL-terminated string. Matches the crate's Cargo
/// version. Static lifetime; do NOT free.
#[no_mangle]
pub extern "C" fn coral_version() -> *const c_char {
    // Embed the version at compile time as a proper C-string literal.
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr() as *const c_char
}

#[cfg(test)]
mod tests {
    use super::*;

    fn to_string(ptr: *mut c_char) -> String {
        assert!(!ptr.is_null(), "FFI returned NULL: {}", error_string());
        let s = unsafe { CStr::from_ptr(ptr).to_str().unwrap().to_owned() };
        unsafe { coral_free_string(ptr) };
        s
    }

    fn error_string() -> String {
        let p = coral_last_error();
        if p.is_null() {
            return "<none>".into();
        }
        unsafe { CStr::from_ptr(p).to_string_lossy().into_owned() }
    }

    #[test]
    fn round_trip_nvl() {
        let input = CString::new("SELECT NVL(x, 0) FROM t").unwrap();
        let out = unsafe { coral_translate(input.as_ptr()) };
        assert_eq!(to_string(out), "SELECT COALESCE(x, 0) FROM t");
    }

    #[test]
    fn round_trip_with_type_cast() {
        let input = CString::new("SELECT x::JSONB FROM t").unwrap();
        let out = unsafe { coral_translate(input.as_ptr()) };
        assert_eq!(to_string(out), "SELECT CAST(x AS STRING) FROM t");
    }

    #[test]
    fn null_input_returns_null_and_sets_error() {
        let out = unsafe { coral_translate(std::ptr::null()) };
        assert!(out.is_null());
        assert!(error_string().contains("NULL"));
    }

    #[test]
    fn parse_error_returns_null_and_reports() {
        let input = CString::new("SELEKT * FROM t").unwrap();
        let out = unsafe { coral_translate(input.as_ptr()) };
        assert!(out.is_null());
        let err = error_string();
        assert!(err.contains("parse error"), "err: {err}");
    }

    #[test]
    fn error_cleared_on_success_after_failure() {
        let bad = CString::new("SELEKT x").unwrap();
        let _ = unsafe { coral_translate(bad.as_ptr()) };
        assert!(!error_string().contains("<none>"));
        let good = CString::new("SELECT x FROM t").unwrap();
        let out = unsafe { coral_translate(good.as_ptr()) };
        let _ = to_string(out);
        // Success path cleared the thread-local error.
        let p = coral_last_error();
        assert!(
            p.is_null(),
            "expected cleared error, got: {}",
            error_string()
        );
    }

    #[test]
    fn free_null_is_safe() {
        unsafe { coral_free_string(std::ptr::null_mut()) };
    }

    #[test]
    fn version_is_non_empty() {
        let p = coral_version();
        assert!(!p.is_null());
        let v = unsafe { CStr::from_ptr(p).to_str().unwrap() };
        assert!(v.starts_with("0."), "unexpected version: {v}");
    }
}
