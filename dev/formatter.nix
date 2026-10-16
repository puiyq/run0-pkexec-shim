{ inputs, ... }:
{
  imports = [ inputs.treefmt-nix.flakeModule ];

  perSystem =
    { pkgs, ... }:
    {
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
}
