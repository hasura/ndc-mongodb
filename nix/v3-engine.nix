# Dependencies and build configuration for the v3-engine crate.
#
# To add runtime library dependencies, add packge names to the argument set
# here, and add the same name to the `buildInputs` list below.
#
# To add buildtime dependencies, add packge names to the argument set
# here, and add the same name to the `nativeBuildInputs` list below.
#
# To add environment variables add attributes to `buildArgs`.
#
# To set Cargo options, or other configuration see the Crane documentation,
# https://crane.dev/API.html#cranelibbuildpackage
#
{ src

  # The following arguments come from nixpkgs, and are automatically populated
  # by `callPackage`.
, callPackage
, craneLib
, git
, openssl
, pkg-config
, protobuf
}:

let
  boilerplate = callPackage ./cargo-boilerplate.nix { };
  recursiveMerge = callPackage ./recursiveMerge.nix { };

  buildArgs = recursiveMerge [
    boilerplate.buildArgs
    {
      inherit src;

      # craneLib wants a name for the workspace root
      pname = "v3-engine-workspace";
      version = "3.0.0";

      buildInputs = [
        openssl
      ];

      nativeBuildInputs = [
        git # called by `engine/build.rs` # called by `engine/build.rs`
        pkg-config
        protobuf # required by opentelemetry-proto, a dependency of axum-tracing-opentelemetry
      ];

      doCheck = false;
    }
  ];

  cargoArtifacts = craneLib.buildDepsOnly buildArgs;
in
craneLib.buildPackage
  (buildArgs // {
    inherit cargoArtifacts;

    # The engine's `build.rs` script does a git hash lookup when building in
    # release mode that fails if building with nix.
    CARGO_PROFILE = "dev";
  })
