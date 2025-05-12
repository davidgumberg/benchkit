{
  description = "Benchkit shell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/24.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
        flake-utils.follows = "flake-utils";
      };
    };
  };

  outputs = { nixpkgs, flake-utils, rust-overlay, ... }:
    flake-utils.lib.eachDefaultSystem
      (system:
        let
          overlays = [ (import rust-overlay) ];
          pkgs = import nixpkgs { inherit system overlays; };
          lib = pkgs.lib;
        in
        with pkgs;
        {
          devShells.default = pkgs.mkShell {
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
              rust-bin.stable.latest.default
              sqlite
              zeromq
            ];
            LD_LIBRARY_PATH = lib.makeLibraryPath [ pkgs.gcc14.cc pkgs.capnproto ];
            LOCALE_ARCHIVE = "${pkgs.glibcLocales}/lib/locale/locale-archive";
          };
        }
      );
}
