# coral-ffi

C ABI bindings for [`coral-core`](https://crates.io/crates/coral-core) — call the GaussDB → Spark SQL translator from Python, Go, Node, Ruby, or plain C.

Produces three crate types on build:
- `cdylib` (`libcoral_ffi.so` / `.dylib` / `.dll`) — for FFI at runtime
- `staticlib` (`libcoral_ffi.a`) — for linking directly into a C binary
- `rlib` — so `cargo test` can exercise the FFI from Rust

## C API (4 functions)

```c
#include "coral.h"

char       *coral_translate(const char *input);   // owned, free with coral_free_string
void        coral_free_string(char *ptr);
const char *coral_last_error(void);               // thread-local, borrowed
const char *coral_version(void);                  // static, borrowed
```

Header file: `include/coral.h`.

## Language examples

- **Python** (stdlib `ctypes` only): `examples/python/coral.py` + `examples/python/smoke_demo.py`
- **C**: `examples/c/smoke.c`

See the top-level README at <https://github.com/keaidada/coral/tree/coral-rust/coral-rust> for full protocol and build instructions.

## License

BSD-2-Clause
