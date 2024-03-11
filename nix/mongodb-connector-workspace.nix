# Dependencies and build configuration for the mongo-agent crate.
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
# This expression is intended to be called using `pkgs.callPackage` which
# automatically fills in arguments from nixpkgs in a way that provides the
# correct dependency for the build or host platform.
#
# To cross compile call this using a nixpkgs instance configured for
# cross-compilation:
#
#     let
#       pkgs = import nixpkgs {
#         localSystem = "aarch64-darwin";
#         crossSystem = "aarch64-linux";
#       };
#     in
#       pkgs.callPackage ./mongodb-connector-workspace.nix { }
#
#
{ package ? null # leave as null to build or test all packages
, profile ? "release" # "dev", "release", "test", or "bench"
, staticallyLinked ? false

  # The following arguments come from nixpkgs, and are automatically populated
  # by `callPackage`.
, callPackage
, lib
, openssl
, pkgsStatic
, pkg-config
, protobuf
}:

let
  boilerplate = callPackage ./cargo-boilerplate.nix { inherit staticallyLinked; };
  recursiveMerge = callPackage ./recursiveMerge.nix { };

  # Copy attributes from `boilerplate` to local variables.
  #
  # When statically linking we need to get a copy of craneLib that adds support
  # for a `musl` target.
  inherit (boilerplate) craneLib;

  src =
    let
      jsonFilter = path: _type: builtins.match ".*json" path != null;
      cargoOrJson = path: type:
        (jsonFilter path type) || (craneLib.filterCargoSources path type);
    in
    lib.cleanSourceWith { src = craneLib.path ./..; filter = cargoOrJson; };

  buildArgs = recursiveMerge [
    boilerplate.buildArgs
    ({
      inherit src;

      pname = if package != null then package else "mongodb-connector-workspace";

      # buildInputs are compiled for the target platform that we are compiling for
      buildInputs = [
        openssl
      ];

      # nativeBuildInputs are intended to run on the platform we are building on,
      # as opposed to the platform we are targetting for compilation
      nativeBuildInputs = [
        pkg-config # required for non-static builds
        protobuf # required by opentelemetry-proto, a dependency of axum-tracing-opentelemetry
      ];

      CARGO_PROFILE = profile;
      cargoExtraArgs =
        if package == null
        then "--locked"
        else "--locked --package ${package}";

    } // lib.optionalAttrs staticallyLinked {
      # Configure openssl-sys for static linking. The build script for the
      # openssl-sys crate requires openssl lib and include locations to be
      # specified explicitly for this case.
      #
      # `pkgsStatic` provides versions of nixpkgs that are compiled with musl
      OPENSSL_STATIC = "1";
      OPENSSL_LIB_DIR = "${pkgsStatic.openssl.out}/lib";
      OPENSSL_INCLUDE_DIR = "${pkgsStatic.openssl.dev}/include";
    })
  ];

  # Build project dependencies separately so that we can reuse the cached output
  # when project code changes, but dependencies do not.
  cargoArtifacts = craneLib.buildDepsOnly (buildArgs // { doCheck = false; });

  crate = craneLib.buildPackage
    (buildArgs // {
      inherit cargoArtifacts; # Hook up cached dependencies
      doCheck = false;
    });
in
crate.overrideAttrs (prev: {
  # Add buildArgs to the returned derivation so that we can access it from the
  # caller. cargoArtifacts and src are included automatically.
  passthru.buildArgs = buildArgs // { inherit cargoArtifacts; };
})
