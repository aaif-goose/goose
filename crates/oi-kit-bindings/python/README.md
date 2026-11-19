# oi-kit

Python bindings for the Open Intelligence Kit (oi-kit).

This package is generated from the Rust `oi-kit-bindings` crate using UniFFI.

## Build a local wheel

From the repository root:

```bash
just --justfile crates/oi-kit-bindings/justfile python-wheel
```

The wheel is written to `crates/oi-kit-bindings/python/dist/`.
