{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    inputs:
    let
      system = "x86_64-linux";
      pkgs = inputs.nixpkgs.legacyPackages.${system};
    in
    {
      packages.${pkgs.stdenv.hostPlatform.system}.default = pkgs.callPackage ./package.nix { };
      devShells.${pkgs.stdenv.hostPlatform.system}.default =
        pkgs.mkShell.override { stdenv = pkgs.clangStdenv; }
          {
            packages = with pkgs; [
              cargo
              rustc
              rustfmt
              clippy
              rust-analyzer
            ];
          };
    };
}
