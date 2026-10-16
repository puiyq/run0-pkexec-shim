{
  inputs = {
    nixpkgs.url = "https://channels.nixos.org/nixos-unstable/nixexprs.tar.zst";
    flake-parts = {
      url = "github:hercules-ci/flake-parts";
      inputs.nixpkgs-lib.follows = "nixpkgs";
    };
  };

  outputs =
    inputs@{ self, ... }:
    inputs.flake-parts.lib.mkFlake { inherit inputs; } {
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      imports = [ ./dev/partition.nix ];

      perSystem =
        { pkgs, self', ... }:
        {
          packages = {
            "run0-pkexec-shim" = pkgs.callPackage ./package.nix { };
            default = self'.packages."run0-pkexec-shim";
          };

          devShells.default = pkgs.mkShell {
            inputsFrom = [ self'.packages.default ];
            packages = with pkgs; [
              rust-analyzer
              clippy
              rustfmt
            ];
          };
        };

      flake = {
        overlays.default = final: _prev: {
          run0-pkexec-shim = self.packages.${final.stdenv.hostPlatform.system}.default;
        };

        nixosModules.default =
          {
            pkgs,
            lib,
            config,
            ...
          }:
          let
            cfg = config.security.run0-pkexec-shim;
          in
          {
            options.security = {
              run0-pkexec-shim = {
                enable = lib.mkEnableOption "run0-pkexec-shim instead of setuid pkexec";
                package = lib.mkPackageOption pkgs "run0-pkexec-shim" { } // {
                  # should be removed when upstreaming to nixpkgs
                  default = pkgs.run0-pkexec-shim or self.packages.${pkgs.stdenv.hostPlatform.system}.default;
                };
              };
            };

            config = lib.mkIf cfg.enable {
              security = {
                polkit.enable = true;
                wrappers.pkexec = {
                  setuid = lib.mkForce false;
                  source = lib.mkForce (lib.getExe cfg.package);
                };
              };
            };
          };
      };
    };
}
