{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        mkShell = pkgs.mkShell.override {
          stdenv = pkgs.stdenv.override {
            preHook = "";
            allowedRequisites = null;
            initialPath = pkgs.lib.filter (
              pkg: pkgs.lib.hasPrefix "coreutils" pkg.name
            ) pkgs.stdenvNoCC.initialPath;
            extraNativeBuildInputs = [ ];
          };
        };
      in
      {
        devShells.default = mkShell {
          nativeBuildInputs = with pkgs; [
            rustc
            cargo
            rustfmt
            clippy
            rust-analyzer
            cargo-tarpaulin
          ];
          RUST_SRC_PATH = "${pkgs.rust.packages.stable.rustPlatform.rustLibSrc}";
        };
      }
    );
}
