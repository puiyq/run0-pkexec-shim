{ inputs, ... }:
{
  imports = [ inputs.flake-parts.flakeModules.partitions ];

  partitionedAttrs.formatter = "formatter";

  partitions.formatter = {
    extraInputsFlake = ./.;
    module = ./formatter.nix;
  };
}
