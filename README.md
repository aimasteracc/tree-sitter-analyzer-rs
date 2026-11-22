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
python3 tools/check_contract_status.py
```

The GitHub Actions workflow runs the smoke suite (must be green) and `tools/check_contract_status.py`, which checks each contract test against `tools/pending-contract-tests.txt`. Pending tests must fail only on `todo!()` / `unimplemented!()`; implemented tests must pass. An unexpected pass, a non-todo failure, or a failure of a test not on the list fails CI.

## Layout

- `tsa-core/` — Rust crate (`publish = false`)
- `fixtures/` — v3.1.2 golden JSON plus regeneration scripts
- `tests/golden/` — vendored TSA v3.1.2 Python corpus and locked `corpus_python_expected.json`

This repository was split from the `rust/` directory of TSA branch `feature/rust-anything-rewrite` (head `632d2ba2`). Paths in older commits that start with `rust/` should be read as repository-root paths here.
