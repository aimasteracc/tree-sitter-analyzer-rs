# tree-sitter-analyzer-rs

Experimental Rust rewrite of [tree-sitter-analyzer](https://github.com/aimasteracc/tree-sitter-analyzer) (TSA), using [rust-anything](https://github.com/aimasteracc/rust-anything) unit-by-unit, with behavior checked against recorded v3.1.2 output.

**This is not an official TSA release.** It is not published to crates.io, PyPI, or npm. The supported user-facing package remains PyPI `tree-sitter-analyzer`.

## Current status (Stage 0–1)

- Spec: [`SPEC.md`](SPEC.md)
- Golden fixtures in `fixtures/` were recorded from a real install of PyPI `tree-sitter-analyzer==3.1.2` (see `fixtures/MANIFEST.json` and `fixtures/tools/`)
- Scope so far: JSON envelope, MCP facade dispatch, and the Python language plugin
- `cargo test` at the repository root:
  - `fixtures_smoke` should pass (14 tests; fixtures parse)
  - contract tests should fail on `todo!("... not yet implemented in Rust")` (30 tests: envelope 13, facade 11, python_plugin 6)

```bash
cargo test --test fixtures_smoke
cargo test --test envelope_contract --test facade_contract --test python_plugin_contract
```

The GitHub Actions workflow runs the smoke suite (must be green) and checks that the contract suite is still red for the unimplemented stubs, not because of compile or fixture errors.

## Layout

- `tsa-core/` — Rust crate (`publish = false`)
- `fixtures/` — v3.1.2 golden JSON plus regeneration scripts
- `tests/golden/corpus_python.py` — vendored copy of the TSA v3.1.2 Python corpus used by the contract tests

This repository was split from the `rust/` directory of TSA branch `feature/rust-anything-rewrite` (head `632d2ba2`). Paths in older commits that start with `rust/` should be read as repository-root paths here.
