#!/usr/bin/env bash
set -ex

# Nix installation (Package manager)
mkdir -p /nix && chown kasm-user:kasm-user /nix
su - kasm-user -c "curl -L https://nixos.org/nix/install | sh -s -- --no-daemon"
su - kasm-user -c "mkdir -p ~/.config/nix && echo 'experimental-features = nix-command flakes' >> ~/.config/nix/nix.conf"