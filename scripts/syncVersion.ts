/**
 * @file Sync version from `frontends/wasm/package.json` to `Cargo.toml` (`workspace.package.version` and internal workspace dependency versions), useful in CI.
 * Run with `bun scripts/syncVersion.ts`
 */

const pkg = await Bun.file("frontends/wasm/package.json").json();
const version = pkg.version;

const cargo = await Bun.file("Cargo.toml").text();

const updatedCargo = cargo
  // Sync workspace.package.version
  .replace(/^version\s*=\s*"[^"]*"/m, `version = "${version}"`)
  // Sync internal workspace dependency versions (lines with both `path` and `version`)
  .replace(
    /^(.+path\s*=\s*"[^"]*".+)version\s*=\s*"[^"]*"/gm,
    `$1version = "${version}"`,
  );

await Bun.write("Cargo.toml", updatedCargo);

// biome-ignore lint/suspicious/noConsole: CLI script
console.log(`Synced version ${version} to Cargo.toml`);

export {};
