{
  description = "Minimal self-hosted Git forge";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        gitbox = pkgs.rustPlatform.buildRustPackage {
          pname = "gitbox";
          version = "0.1.0";
          src = ./.;

          cargoLock.lockFile = ./Cargo.lock;

          nativeBuildInputs = [ pkgs.makeWrapper ];
          nativeCheckInputs = [ pkgs.git ];

          postInstall = ''
            wrapProgram $out/bin/gitbox \
              --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.git ]}
          '';

          meta.mainProgram = "gitbox";
        };
      in
      {
        packages = {
          default = gitbox;
          inherit gitbox;
        };

        checks.default = gitbox;

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
    );
}
