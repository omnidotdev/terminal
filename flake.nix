{
  description = "Omni Terminal | A GPU-accelerated terminal emulator";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts.url = "github:hercules-ci/flake-parts";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
    systems.url = "github:nix-systems/default";
  };

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [flake-parts.flakeModules.easyOverlay];

      systems = import inputs.systems;

      perSystem = {
        self',
        inputs',
        pkgs,
        system,
        lib,
        ...
      }: let
        mkDevShell = rust-toolchain: let
          runtimeDeps = self'.packages.omni-terminal.runtimeDependencies;
          tools =
            self'.packages.omni-terminal.nativeBuildInputs ++ self'.packages.omni-terminal.buildInputs ++ [rust-toolchain];
        in
          pkgs.mkShell {
            packages = [self'.formatter] ++ tools;
            LD_LIBRARY_PATH = "${lib.makeLibraryPath runtimeDeps}";
          };
        toolchains = rec {
          msrv = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          stable = pkgs.rust-bin.stable.latest.minimal;
          nightly = pkgs.rust-bin.selectLatestNightlyWith (toolchain: toolchain.minimal);
          omni-terminal = msrv;
          default = omni-terminal;
        };
      in {
        formatter = pkgs.alejandra;
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [(import inputs.rust-overlay)];
        };

        overlayAttrs = {inherit (self'.packages) omni-terminal;};
        packages =
          lib.mapAttrs' (
            k: v: {
              name =
                if builtins.elem k ["omni-terminal" "default"]
                then k
                else "omni-terminal-${k}";
              value = pkgs.callPackage ./pkgOmniTerminal.nix {rust-toolchain = v;};
            }
          )
          toolchains;
        devShells = lib.mapAttrs (_: v: mkDevShell v) toolchains;
      };
    };
}
