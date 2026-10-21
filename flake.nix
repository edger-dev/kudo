{
  description = "Kudo — composable platform for human-agent workflows";

  inputs.jig.url = "github:edger-dev/jig";

  outputs = { self, jig }:
    jig.lib.mkWorkspace
      {
        pname = "kudo";
        src = ./.;
        # The console (crates/kudo-console) is Dioxus fullstack: dx builds its
        # wasm UI; wasm-bindgen matches the crate's own version.
        extraDevPackages = pkgs: [ pkgs.dioxus-cli pkgs.wasm-bindgen-cli ];
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
