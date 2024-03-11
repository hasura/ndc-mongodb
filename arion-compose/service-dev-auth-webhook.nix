{ pkgs }:

{
  service = {
    useHostStore = true;
    # Get node from a Docker image instead of from Nix because cross-compiling
    # Node from Darwin to Linux doesn't work.
    image = "node:lts-alpine";
    command = [
      "node"
      "${pkgs.pkgsCross.linux.dev-auth-webhook}/index.js"
    ];
  };
}
