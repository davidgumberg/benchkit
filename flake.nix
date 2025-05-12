{
  description = "Benchkit shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.11";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachSystem [ "x86_64-linux"] (system:
      let
        pkgs = import nixpkgs { inherit system; };
        lib = pkgs.lib;

        nativeBuildInputs = with pkgs; [
          byacc
          ccache
          cmake
          gcc14
          gnum4
          gnumake
          pkg-config
        ];

        buildInputs = with pkgs; [
          boost
          capnproto
          libevent
          hyperfine
          sqlite
          zeromq
        ];

      in
      {
        devShells.default = pkgs.mkShell {
          LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.gcc14.cc pkgs.capnproto ];
          LOCALE_ARCHIVE = "${pkgs.glibcLocales}/lib/locale/locale-archive";
          inherit nativeBuildInputs buildInputs;
        };
      }
    );
}
