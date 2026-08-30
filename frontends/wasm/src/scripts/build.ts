import { $ } from "bun";

const { log, warn, error } = console;
const WORKSPACE_ROOT = "../..";

const build = async () => {
  await $`rm -rf build`;
  await $`mkdir -p build`;

  log("Building WASM...");
  await $`cargo build -p omni-terminal-wasm --target wasm32-unknown-unknown --release`;

  log("Running wasm-bindgen...");
  // Prefer cargo-installed wasm-bindgen over system version for version alignment
  const home = process.env.HOME ?? "~";
  const wasmBindgen = `${home}/.cargo/bin/wasm-bindgen`;
  await $`${wasmBindgen} ${WORKSPACE_ROOT}/target/wasm32-unknown-unknown/release/omni_terminal_wasm.wasm --out-dir build --target web`;

  // wasm-opt is intentionally DISABLED. `wasm-opt -O` (Binaryen v123) produces a
  // module that passes validation but renders blank at runtime for this crate
  // (the wgpu/WebGPU path); the release cargo build without it renders correctly
  // and is only ~350KB larger. Re-enable only with a Binaryen config verified to
  // render in a browser (test via `just web-serve`), never by size alone.
  warn("wasm-opt disabled: -O corrupts the wgpu WASM at runtime (renders blank)");

  log("Compiling TypeScript...");
  await $`bunx tsc --noEmit false --declaration --emitDeclarationOnly --outDir build`;

  // Emit JS alongside declarations -- keep WASM import as external
  await Bun.build({
    entrypoints: ["src/index.ts"],
    outdir: "build",
    target: "browser",
    external: ["./omni_terminal_wasm.js"],
  });

  log("Build complete.");
};

build().catch((err) => {
  error(err);
  process.exit(1);
});
