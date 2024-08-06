# Provides boilerplate configurating for cross-compiling and for
# statically-linking Rust programs.
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
#       pkgs.callPackage ./cargo-boilerplate.nix { }
#
{
  # Note that we can statically-link if the cross-compilation target is Linux,
  # and the build system is 64-bit because these are the conditions where Nix
  # has musl packages available.
  staticallyLinked ? false

  # The following arguments come from nixpkgs, and are automatically populated
  # by `callPackage`.
, craneLib # added by overlay in flake.nix
, darwin
, libiconv
, lib
, pkgsStatic
, rustToolchain # added by overlay in flake.nix
, stdenv
}:

let
  # `hostPlatform` is the cross-compilation output platform;
  # `buildPlatform` is the platform we are compiling on
  buildPlatform = stdenv.buildPlatform;
  hostPlatform = stdenv.hostPlatform;

  # `config` is a cpu-vendor-os-abi string like "aarch64-unknown-linux-gnu". To
  # statically link we need to change the ABI from "gnu" to "musl".
  #
  # Ideally we would pass in a crossSystem argument of the form
  # "aarch64-unknown-linux-musl" instead of doing this fixup. But that doesn't
  # seem to work.
  buildTarget =
    if staticallyLinked then builtins.replaceStrings [ "gnu" ] [ "musl" ] hostPlatform.config
    else hostPlatform.config;

  # Make sure the Rust toolchain includes support for the platform we are
  # building for in case we are cross-compiling. In practice this is only
  # necessary if we are statically linking, and therefore have a `musl` target.
  # But it doesn't hurt anything to make this override in other cases.
  toolchain = pkgs: pkgs.rustToolchain.override { targets = [ buildTarget ]; };

  # Converts host system string for use in environment variable names
  envCase = triple: lib.strings.toUpper (builtins.replaceStrings [ "-" ] [ "_" ] triple);
in
{
  buildArgs = {
    # buildInputs are compiled for the target platform that we are compiling for
    buildInputs = lib.optionals hostPlatform.isDarwin [
      darwin.apple_sdk.frameworks.CoreServices # needed to run mongodb-connector unit tests
      pkgsStatic.darwin.apple_sdk.frameworks.Security
      pkgsStatic.darwin.apple_sdk.frameworks.SystemConfiguration
    ];

    # nativeBuildInputs are intended to run on the platform we are building on,
    # as opposed to the platform we are targetting for compilation
    nativeBuildInputs = lib.optionals buildPlatform.isDarwin [
      libiconv
    ];

    CARGO_BUILD_TARGET = buildTarget;
    "CARGO_TARGET_${envCase buildTarget}_LINKER" = "${stdenv.cc.targetPrefix}cc";

    # This environment variable may be necessary if any of your dependencies use
    # a build-script which invokes the `cc` crate to build some other code. The
    # `cc` crate should automatically pick up on our target-specific linker
    # above, but this may be necessary if the build script needs to compile and
    # run some extra code on the build system.
    HOST_CC = "${stdenv.cc.nativePrefix}cc";
  }
  // lib.optionalAttrs staticallyLinked {
    CARGO_BUILD_RUSTFLAGS = "-C target-feature=+crt-static";
  };

  craneLib = craneLib.overrideToolchain toolchain;
}
