{ callPackage
, craneLib
, jq
, makeWrapper
}:

let
  workspace = callPackage ./mongodb-connector-workspace.nix { };
in
craneLib.buildPackage
  (workspace.buildArgs // {
    pname = "mongodb-connector-integration-tests";

    doCheck = false;

    # craneLib passes `--locked` by default - this is necessary for
    # repdroducible builds.
    #
    # `--tests` builds an executable to run tests instead of compiling
    # `main.rs`
    #
    # Integration tests are disabled by default - `--features integration`
    # enables them.
    #
    # We only want the integration tests so we're limiting to building the test
    # runner for that crate.
    cargoExtraArgs = "--locked --tests --package integration-tests --features integration";

    # Add programs we need for postInstall hook to nativeBuildInputs
    nativeBuildInputs = workspace.buildArgs.nativeBuildInputs ++ [
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
        --set-default INSTA_WORKSPACE_ROOT "${./..}"
    '';
  })

