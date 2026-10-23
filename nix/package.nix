{
  lib,
  rustPlatform,
  pkg-config,
  udev,
  autoAddDriverRunpath,
}:

let
  workspace = (lib.importTOML ../Cargo.toml).workspace.package;
in
rustPlatform.buildRustPackage {
  pname = "gale";
  inherit (workspace) version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../crates
      ../assets
      ../ui/dist
      ../packaging/config.example.toml
      ../packaging/udev
    ];
  };

  cargoLock.lockFile = ../Cargo.lock;

  cargoBuildFlags = [
    "--package=galed"
    "--package=gale-cli"
  ];

  nativeBuildInputs = [
    pkg-config
    autoAddDriverRunpath
  ];

  buildInputs = [ udev ];

  postInstall = ''
    install -Dm644 packaging/udev/60-gale-corsair.rules -t $out/lib/udev/rules.d
  '';

  meta = {
    description = "API-first fan control daemon";
    homepage = "https://github.com/Garulf/gale";
    license = lib.licenses.gpl3Plus;
    mainProgram = "galed";
    platforms = lib.platforms.linux;
  };
}
