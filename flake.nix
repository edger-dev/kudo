{
  description = "Kudo — composable platform for human-agent workflows";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  inputs.jig.url = "github:edger-dev/jig";
  inputs.jig.inputs.nixpkgs.follows = "nixpkgs";
  inputs.nixpkgs-dioxus074.url = "github:NixOS/nixpkgs/8d8c1fa5b412c223ffa47410867813290cdedfef";

  outputs = { self, jig, nixpkgs, nixpkgs-dioxus074, ... }:
    let
      workspace = jig.lib.mkWorkspace
      {
        pname = "kudo";
        src = ./.;
        # The console (crates/kudo-console) is Dioxus fullstack: dx builds its
        # wasm UI, and its wasm-bindgen must be the exact version the lockfile
        # resolved. nixpkgs' dx carries an older one at the *end* of its PATH,
        # so this one, earlier, wins. Bump the version and both hashes with
        # the lockfile's wasm-bindgen.
        extraDevPackages = pkgs:
          let
            dx074 = import nixpkgs-dioxus074 {
              system = pkgs.stdenv.hostPlatform.system;
            };
            wasm-bindgen-cli = pkgs.buildWasmBindgenCli rec {
              src = pkgs.fetchCrate {
                pname = "wasm-bindgen-cli";
                version = "0.2.126";
                hash = "sha256-H6Is3fiZVxZCfOMWK5dWMSrtn50VGv0sfdnsT+cTtyk=";
              };
              cargoDeps = pkgs.rustPlatform.fetchCargoVendor {
                inherit src;
                inherit (src) pname version;
                hash = "sha256-VucqkXbCi4qtQzY/HrXiDnbSURsagPsdNVMn1Tw3UiY=";
              };
            };
          in
          [ wasm-bindgen-cli dx074.dioxus-cli pkgs.binaryen ];
      }
      {
        rust = {
          extraSrc = [
            "docs"
            "crates/kudo-node/packaging"
            "moco-processes.styx"
          ];
          wasm = true;
        };
        docs = { };
        kinora = { };
      };
      # The worktree integration tests invoke git.
      withGit = system: package: package.overrideAttrs (old: {
        nativeCheckInputs = (old.nativeCheckInputs or [])
          ++ [ nixpkgs.legacyPackages.${system}.git ];
      });
    in
    workspace // {
      packages = builtins.mapAttrs (system: packages:
        let
          kudo = withGit system packages.kudo;
        in
        packages // { inherit kudo; default = kudo; }
      ) workspace.packages;

      checks = builtins.mapAttrs (system: checks:
        checks // { build = withGit system checks.build; }
      ) workspace.checks;

      # kudo-node as a home-manager user service; see the module for options.
      homeModules.kudo-node = { lib, pkgs, ... }: {
        imports = [ ./crates/kudo-node/packaging/home-module.nix ];
        services.kudo-node.package = lib.mkDefault
          self.packages.${pkgs.stdenv.hostPlatform.system}.kudo;
      };
    };
}
