/* Copyright 2026 coral-rust contributors
 * Licensed under the BSD-2-Clause license.
 *
 * C header for libcoral_ffi (coral-rust / coral-core).
 *
 * Link against libcoral_ffi.so / .dylib / .dll (cdylib) or libcoral_ffi.a
 * (staticlib). Both are produced by `cargo build -p coral-ffi`.
 *
 * Memory rules:
 *   - Inputs are borrowed (caller retains ownership, must outlive the call).
 *   - coral_translate() returns an owned char* — caller MUST pass it to
 *     coral_free_string() to release it.
 *   - coral_last_error() returns a pointer valid until the next
 *     coral_translate() call on the same thread. Do NOT free it.
 *   - coral_version() returns a static pointer. Do NOT free it.
 */

#ifndef CORAL_H
#define CORAL_H

#ifdef __cplusplus
extern "C" {
#endif

/**
 * Translate a GaussDB / openGauss SQL statement to Spark SQL.
 *
 * @param input  NUL-terminated UTF-8 SQL string. Must not be NULL.
 * @return       Owned char* on success (free with coral_free_string); NULL
 *               on error (check coral_last_error() on the same thread).
 */
char *coral_translate(const char *input);

/**
 * Free a string previously returned by coral_translate(). NULL is a no-op.
 * Do NOT call this on any other pointer.
 */
void coral_free_string(char *ptr);

/**
 * Get the last error message on this thread, or NULL if the most recent
 * call succeeded. The returned pointer is valid until the next
 * coral_translate() call on the same thread.
 */
const char *coral_last_error(void);

/**
 * Library version string (e.g. "0.1.0"). Static lifetime; do NOT free.
 */
const char *coral_version(void);

#ifdef __cplusplus
} /* extern "C" */
#endif

#endif /* CORAL_H */
