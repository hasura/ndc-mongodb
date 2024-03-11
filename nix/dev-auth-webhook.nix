# Used to fake auth checks when running v3-engine locally.
#
# Creates a derivation that includes `index.js` and `node_modules`. To run it
# use a command like,
#
#     node ${pkgs.dev-auth-webhook}/index.js
#
{ src

  # The following arguments come from nixpkgs, and are automatically populated
  # by `callPackage`.
, fetchNpmDeps
, nodejs
, stdenvNoCC
}:

let
  npmDeps = fetchNpmDeps {
    inherit src;
    name = "dev-auth-webhook-npm-deps";
    hash = "sha256-s2s5JeaiUsh0mYqh5BYfZ7uEnsPv2YzpOUQoMZj1MR0=";
  };
in
stdenvNoCC.mkDerivation {
  inherit src;
  name = "dev-auth-webhook";
  nativeBuildInputs = [ nodejs ];
  buildPhase = ''
    npm install --cache "${npmDeps}"
  '';
  installPhase = ''
    mkdir -p "$out"
    cp index.js "$out/"
    cp -r node_modules "$out/"
  '';
}
