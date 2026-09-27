{
  description = "Minimal self-hosted Git forge";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachSystem [ "aarch64-darwin" "aarch64-linux" "x86_64-linux" ] (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        gitbox = pkgs.callPackage ./nix/package.nix { };
      in
      {
        packages = {
          default = gitbox;
          inherit gitbox;
        };

        checks = {
          default = gitbox;
        }
        // pkgs.lib.optionalAttrs pkgs.stdenv.hostPlatform.isLinux {
          nixos = import ./nix/tests/nixos.nix { inherit pkgs gitbox; };
        };
        formatter = pkgs.nixfmt;

        apps.default = flake-utils.lib.mkApp {
          drv = gitbox;
        };

        devShells.default = pkgs.mkShell {
          packages = with pkgs; [
            cargo
            clippy
            git
            nil
            nixd
            nixfmt
            openssl
            openssh
            pkg-config
            protobuf
            rust-analyzer
            rustc
            rustfmt
            shellcheck
          ];

          env = {
            RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          };
        };
      }
    )
    // {
      nixosModules.default = import ./nix/nixos.nix;
      homeManagerModules.default = import ./nix/home-manager.nix;
      overlays.default = final: prev: {
        gitbox = final.callPackage ./nix/package.nix { };
      };
    };
}
