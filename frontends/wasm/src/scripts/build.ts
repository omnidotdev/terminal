import { $ } from "bun";

import type { ShellError } from "bun";

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

  // wasm-opt is optional (may not be installed)
  try {
    log("Optimizing WASM binary...");
    await $`wasm-opt -O build/omni_terminal_wasm_bg.wasm -o build/omni_terminal_wasm_bg.wasm`;
  } catch (_e: unknown) {
    const err = _e as ShellError;
    if (err.stderr?.toString().includes("not found")) {
      warn("wasm-opt not found, skipping optimization");
    } else {
      throw err;
    }
  }

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
