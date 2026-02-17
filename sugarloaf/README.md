# Sugarloaf

GPU rendering engine for [Omni Terminal](https://github.com/omnidotdev/terminal), built on WebGPU. Targets desktop (native Rust) and web (WebAssembly).

> Originally forked from [Rio Terminal](https://github.com/raphamorim/rio)'s Sugarloaf by [Raphael Amorim](https://github.com/raphamorim), licensed under MIT.

```bash
cargo run --example text
```

## WASM Tests

### Setup

Install `wasm-bindgen-cli` globally:

```bash
cargo install wasm-bindgen-cli
```

### Running Tests

From the root sugarloaf directory:

```bash
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner cargo test --target wasm32-unknown-unknown -p sugarloaf --tests
```

Flag explanation:

- `CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner` -- use the test harness provided by `wasm-bindgen-cli`
- `-p sugarloaf` -- only run tests in the sugarloaf package
- `--tests` -- only run tests, skip examples (some don't compile to WASM)
