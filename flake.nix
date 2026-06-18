{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
    treefmt-nix = {
      url = "github:numtide/treefmt-nix";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ self, ... }:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      imports = [ inputs.treefmt-nix.flakeModule ];

      perSystem =
        { pkgs, self', ... }:
        {
          packages = {
            "run0-pkexec-shim" = pkgs.callPackage ./package.nix { };
            default = self'.packages."run0-pkexec-shim";
          };

          devShells.default = pkgs.mkShell.override { stdenv = pkgs.clangStdenv; } {
            inputsFrom = [ self'.packages.default ];
            packages = with pkgs; [
              rust-analyzer
            ];
          };
          treefmt = {
            projectRootFile = "flake.nix";
            programs = {
              deadnix.enable = true;
              nixfmt = {
                enable = true;
                package = pkgs.nixfmt-rs;
              };
              rustfmt.enable = true;
              statix.enable = true;
              taplo.enable = true;
            };
          };
        };

      };
    };
}
