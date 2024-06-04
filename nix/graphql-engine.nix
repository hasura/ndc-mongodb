# Dependencies and build configuration for the graphql-engine crate.
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
, package ? null # leave as null to build or test all packages

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

      pname = "graphql-engine-workspace";

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
    pname = if package != null then package else buildArgs.pname;

    cargoExtraArgs =
      if package == null
      then "--locked"
      else "--locked --package ${package}";

    # The engine's `build.rs` script does a git hash lookup when building in
    # release mode that fails if building with nix.
    CARGO_PROFILE = "dev";
  })
