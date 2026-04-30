# coral-sql — Python package

PyPI wheel for the `coral-rust` translator. Ships a pre-compiled
`libcoral_ffi.{so,dylib,dll}` inside the wheel; uses `ctypes` at runtime (no
PyO3, no Rust toolchain needed to install).

## Install (once published)

```bash
pip install coral-sql
```

## Usage

```python
from coral_sql import translate, CoralError

try:
    spark = translate("SELECT NVL(x, 0), y::INT FROM t")
    print(spark)
except CoralError as e:
    print(e)
```

## Build (maintainers)

```bash
cd coral-rust
cargo build --release -p coral-ffi
cd python
pip install build
python -m build --wheel          # produces dist/coral_sql-<ver>-<plat>.whl
```

For cross-platform wheels, use **cibuildwheel** in CI (see
`.github/workflows/coral-rust.yml` once the release workflow is wired up).

The build process copies `../target/release/libcoral_ffi.{so,dylib,dll}` into
the wheel under `coral_sql/_lib/` at build time (see `pyproject.toml` and
`MANIFEST.in`).
