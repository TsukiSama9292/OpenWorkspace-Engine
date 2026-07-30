#!/usr/bin/env bash
set -ex

# Nix installation (Package manager)
mkdir -p /nix && chown ow_user:ow_user /nix
su - ow_user -c "curl -L https://nixos.org/nix/install | sh -s -- --no-daemon"
su - ow_user -c "mkdir -p ~/.config/nix && echo 'experimental-features = nix-command flakes' >> ~/.config/nix/nix.conf"