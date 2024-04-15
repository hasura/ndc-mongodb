# Used to fake auth checks when running graphql-engine locally.
#
{ src

  # The following arguments come from nixpkgs, and are automatically populated
  # by `callPackage`.
, callPackage
, craneLib
}:

let
  boilerplate = callPackage ./cargo-boilerplate.nix { };
  recursiveMerge = callPackage ./recursiveMerge.nix { };

  buildArgs = recursiveMerge [
    boilerplate.buildArgs
    {
      inherit src;
      pname = "dev-auth-webhook";
      version = "3.0.0";
      doCheck = false;
    }
  ];

  cargoArtifacts = craneLib.buildDepsOnly buildArgs;
in
craneLib.buildPackage
  (buildArgs // {
    inherit cargoArtifacts;
  })
