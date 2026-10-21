{
  description = "Kudo — composable platform for human-agent workflows";

  inputs.jig.url = "github:edger-dev/jig";

  outputs = { self, jig }:
    jig.lib.mkWorkspace
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
          [ wasm-bindgen-cli pkgs.dioxus-cli pkgs.binaryen ];
      }
      {
        rust = { wasm = true; };
        docs = { };
        kinora = { };
      }
    // {
      # kudo-node as a home-manager user service; see the module for options.
      #
      # The default package skips the test run: jig's crane source filter keeps
      # only Rust sources, and kudo-node's tests read docs/ and packaging/
      # files the sandbox never sees. The tests run under `cargo test`.
      homeModules.kudo-node = { lib, pkgs, ... }: {
        imports = [ ./crates/kudo-node/packaging/home-module.nix ];
        services.kudo-node.package = lib.mkDefault
          (self.packages.${pkgs.stdenv.hostPlatform.system}.kudo.overrideAttrs { doCheck = false; });
      };
    };
}
