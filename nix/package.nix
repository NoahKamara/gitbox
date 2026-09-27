{
  lib,
  rustPlatform,
  makeWrapper,
  git,
  openssh,
}:
rustPlatform.buildRustPackage {
  pname = "gitbox";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;
  nativeBuildInputs = [ makeWrapper ];
  nativeCheckInputs = [
    git
    openssh
  ];
  postInstall = ''
    for program in gitbox gitboxd; do
      wrapProgram $out/bin/$program \
        --prefix PATH : ${
          lib.makeBinPath [
            git
            openssh
          ]
        }
    done
  '';
  meta = {
    description = "Git repository hosting over restricted SSH";
    mainProgram = "gitbox";
    platforms = lib.platforms.unix;
  };
}
