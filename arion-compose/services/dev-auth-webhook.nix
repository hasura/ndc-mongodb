{ pkgs }:

let
  dev-auth-webhook = pkgs.pkgsCross.linux.dev-auth-webhook;
in
{
  service = {
    useHostStore = true;
    command = [
      "${dev-auth-webhook}/bin/dev-auth-webhook"
    ];
  };
}
