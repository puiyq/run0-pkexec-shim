# run0-pkexec-shim

A lightweight `pkexec` compatibility shim built on top of `run0`, designed for NixOS systems that prefer non-setuid privilege escalation paths.

## Why this exists

Some desktop applications still invoke `pkexec` as a generic PolicyKit entry point for privilege escalation. On NixOS, `pkexec` is typically provided via a setuid wrapper, which is undesirable in hardened configurations.

With systemd 256 introducing `run0`, it becomes possible to delegate privilege escalation to systemd-managed mechanisms rather than setuid binaries in certain workflows.

This project provides a compatibility layer that translates `pkexec` invocations into `run0`-based execution while still relying on Polkit for authentication where applicable.

## Features

- Non-setuid `pkexec` replacement (via NixOS wrapper system)
- Translates common `pkexec` usage into `run0` execution paths
- Designed for integration with Polkit-enabled desktop environments
- NixOS-first packaging with module support

## Important notes

This is a compatibility shim, not a full reimplementation of `pkexec`.

Behavior differences may exist in:

- session handling semantics
- environment propagation
- DBus / Polkit agent expectations
- edge-case pkexec flags and internal behavior

Use in production systems should be done cautiously and tested against your desktop workload.

## NixOS usage

### Flake input

```nix
inputs.run0-pkexec-shim.url = "github:puiyq/run0-pkexec-shim";
```

### Enable module

```nix
{
  imports = [
    inputs.run0-pkexec-shim.nixosModules.default
  ];

  security.run0-pkexec-shim.enable = true;
}
```

When enabled, the module:

- replaces the system `pkexec` wrapper
- integrates with `security.wrappers`
- ensures Polkit is available in the system configuration

You do not need to add the package manually to `systemPackages`.

## Overlay usage

```nix
{
  nixpkgs.overlays = [
    inputs.run0-pkexec-shim.overlays.default
  ];
}
```

Then available as:

```nix
pkgs.run0-pkexec-shim
```

## Build manually

```bash
nix build .#run0-pkexec-shim
```

## Compatibility scope

Expected to work with software that:

- calls `pkexec` as a privilege escalation frontend
- relies on standard Polkit authentication dialogs
- does not depend on undocumented pkexec internals

Known risk areas:

- GNOME/KDE components with tight pkexec coupling
- scripts relying on exact pkexec environment semantics
- non-Polkit-aware invocations

## Development

```bash
nix develop
nix fmt
nix flake check
```

## Acknowledgements

- Inspired by [run0-sudo-shim](https://github.com/LordGrimmauld/run0-sudo-shim)
