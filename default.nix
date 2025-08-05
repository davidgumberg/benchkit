{
  lib,
  rustPlatform,
  pkg-config,
  hwloc,
}: let
  cargoToml = builtins.fromTOML (builtins.readFile ./Cargo.toml);
in
  rustPlatform.buildRustPackage rec {
    pname = cargoToml.package.name;
    version = cargoToml.package.version;

    src = lib.cleanSource ./.;

    cargoLock = {
      lockFile = ./Cargo.lock;
    };

    nativeBuildInputs = [pkg-config];
    buildInputs = [hwloc];

    meta = with lib; {
      description = "Bitcoin benchmarking toolkit";
      license = licenses.mit;
      maintainers = [];
    };
  }
