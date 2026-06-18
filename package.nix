{
  lib,
  rustPlatform,
}:
rustPlatform.buildRustPackage {
  inherit ((lib.importTOML ./Cargo.toml).package) name version;

  src = lib.fileset.toSource {
    root = ./.;
    fileset = lib.fileset.unions [
      ./Cargo.toml
      ./Cargo.lock
      ./tests
      ./src
    ];
  };
  cargoLock.lockFile = ./Cargo.lock;

  __structuredAttrs = true;

  meta = {
    mainProgram = "run0-pkexec-shim";
    description = "Shim for the pkexec command that utilizes run0";
    homepage = "https://github.com/puiyq/run0-pkexec-shim";
    license = lib.licenses.mit;
    maintainers = with lib.maintainers; [ puiyq ];
    platforms = lib.platforms.linux;
  };
}
