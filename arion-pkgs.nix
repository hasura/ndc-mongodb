# Arion is a Nix frontend to docker-compose. This module is a glue layer that
# feeds the `legacyPackages` output from `flake.nix` to `arion-compose.nix`. The
# `arion` executable reads this file automatically.
let
  # This is the fast-and-dirty method for importing a flake into a non-flake Nix
  # expression. If this stops working we can use the more-correct, commented-out
  # definition for `flake` below. But be aware that at the time of this writing
  # it is much slower.
  flake = (import flake-compat { src = ./.; }).defaultNix;
  # NB: this is lazy
  lock = builtins.fromJSON (builtins.readFile ./flake.lock);
  inherit (lock.nodes.flake-compat.locked) owner repo rev narHash;
  flake-compat = builtins.fetchTarball {
    url = "https://github.com/${owner}/${repo}/archive/${rev}.tar.gz";
    sha256 = narHash;
  };

  # This is the correct-but-slow way to import a flake.
  # flake = builtins.getFlake (toString ./.);
in
flake.legacyPackages.${builtins.getEnv "system"}
