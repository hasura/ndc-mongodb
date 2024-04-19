# Dependencies and build configuration for the integration-tests crate
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
{ callPackage
, craneLib
, jq
, makeWrapper
, openssl
, pkg-config
}:

let
  boilerplate = callPackage ./cargo-boilerplate.nix { };
  recursiveMerge = callPackage ./recursiveMerge.nix { };

  buildArgs = recursiveMerge [
    boilerplate.buildArgs
    {
      # Filters source directory to select only files required to build Rust crates.
      # This avoids unnecessary rebuilds when other files in the repo change. 
      src = craneLib.cleanCargoSource (craneLib.path ./..);

      pname = "mongodb-connector-integration-tests";

      buildInputs = [
        openssl
      ];

      nativeBuildInputs = [
        pkg-config
      ];

      doCheck = false;
    }
  ];

  cargoArtifacts = craneLib.buildDepsOnly buildArgs;
in
craneLib.buildPackage
  (buildArgs // {
    inherit cargoArtifacts;

    # craneLib passes `--locked` by default - this is necessary for
    # repdroducible builds.
    #
    # `--tests` builds an executable to run tests instead of compiling
    # `main.rs`
    #
    # We only want the integration tests so we're limiting to building the test
    # runner for that crate.
    cargoExtraArgs = "--locked --test '*' --package integration-tests";

    # Add programs we need for postInstall hook to nativeBuildInputs
    nativeBuildInputs = buildArgs.nativeBuildInputs ++ [
      jq
      makeWrapper
    ];

    # Copy compiled test harness to store path. craneLib automatically filters
    # out test artifacts when installing binaries so we have to do this part
    # ourselves.
    postInstall = ''
      local binaries=$(<"$cargoBuildLog" jq -Rr 'fromjson? | .executable | select(.!= null)')
      local bin="$out/bin/integration-tests"

      for binary in "$binaries"; do
        echo "installing '$binary' to '$bin'"
        mkdir -p "$out/bin"
        cp "$binary" "$bin"
      done

      # Set environment variable to point to source workspace so that `insta`
      # (the Rust snapshot test library) can find snapshot files.
      wrapProgram "$bin" \
        --set-default INSTA_WORKSPACE_ROOT "${./..}" \
        --set-default CI true
    '';
  })

