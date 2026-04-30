/* Copyright 2026 coral-rust contributors
 * Licensed under the BSD-2-Clause license.
 *
 * Minimal C client for coral-ffi. Demonstrates link-time use of the
 * dylib and round-tripping a single query through coral_translate().
 *
 * Build (from coral-rust/):
 *
 *   cargo build -p coral-ffi --release
 *
 *   # macOS:
 *   cc ffi/examples/c/smoke.c -o /tmp/coral_c_smoke \
 *     -I ffi/include -L target/release -lcoral_ffi \
 *     -Wl,-rpath,@loader_path/../../target/release
 *
 *   # Linux: same but  -Wl,-rpath,'$ORIGIN/../../target/release'
 *
 *   /tmp/coral_c_smoke
 */

#include <stdio.h>
#include <stdlib.h>
#include "coral.h"

static int translate_one(const char *input) {
    char *out = coral_translate(input);
    if (!out) {
        const char *err = coral_last_error();
        fprintf(stderr, "translate failed: %s\n", err ? err : "(null)");
        return 1;
    }
    printf("[GaussDB] %s\n", input);
    printf("[Spark]   %s\n\n", out);
    coral_free_string(out);
    return 0;
}

int main(void) {
    printf("coral-ffi v%s\n\n", coral_version());

    int rc = 0;
    rc |= translate_one("SELECT NVL(x, 0) FROM t");
    rc |= translate_one("SELECT y::JSONB FROM t");
    rc |= translate_one("SELECT DECODE(s, 1, 'a', 2, 'b', 'c') FROM t");
    rc |= translate_one("SELECT DISTINCT ON (k) k, v FROM t ORDER BY k, v");
    return rc;
}
