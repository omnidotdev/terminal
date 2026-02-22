import { $, type ShellError } from "bun";

const WORKSPACE_ROOT = "../..";

const build = async () => {
  await $`rm -rf build`;
  await $`mkdir -p build`;

  console.log("Building WASM...");
  await $`cargo build -p omni-terminal-wasm --target wasm32-unknown-unknown --release`;

  console.log("Running wasm-bindgen...");
  // Prefer cargo-installed wasm-bindgen over system version for version alignment
  const home = process.env.HOME ?? "~";
  const wasmBindgen = `${home}/.cargo/bin/wasm-bindgen`;
  await $`${wasmBindgen} ${WORKSPACE_ROOT}/target/wasm32-unknown-unknown/release/omni_terminal_wasm.wasm --out-dir build --target web`;

  // wasm-opt is optional (may not be installed)
  try {
    console.log("Optimizing WASM binary...");
    await $`wasm-opt -O build/omni_terminal_wasm_bg.wasm -o build/omni_terminal_wasm_bg.wasm`;
  } catch (_e: unknown) {
    const err = _e as ShellError;
    if (err.stderr?.toString().includes("not found")) {
      console.warn("wasm-opt not found, skipping optimization");
    } else {
      throw err;
    }
  }

  console.log("Compiling TypeScript...");
  await $`bunx tsc --noEmit false --declaration --emitDeclarationOnly --outDir build`;

  // Emit JS alongside declarations -- keep WASM import as external
  await Bun.build({
    entrypoints: ["src/index.ts"],
    outdir: "build",
    target: "browser",
    external: ["./omni_terminal_wasm.js"],
  });

  console.log("Build complete.");
};

build().catch((err) => {
  console.error(err);
  process.exit(1);
});
