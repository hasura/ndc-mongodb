{ craneLib
, jq
, makeWrapper
, mongodbConnectorWorkspace
}:

craneLib.buildPackage
  (mongodbConnectorWorkspace.buildArgs // {
    pname = "mongodb-connector-relational-tests";

    doCheck = false;

    # Build only the MongoDB-backed relational integration test harness.
    cargoExtraArgs = "--locked --features relational-integration-tests --test relational_integration_tests --package mongodb-agent-common";

    nativeBuildInputs = mongodbConnectorWorkspace.buildArgs.nativeBuildInputs ++ [
      jq
      makeWrapper
    ];

    postInstall = ''
      local binaries=$(<"$cargoBuildLog" jq -Rr 'fromjson? | .executable | select(.!= null)')
      local bin="$out/bin/relational-tests"

      for binary in $binaries; do
        echo "installing '$binary' to '$bin'"
        mkdir -p "$out/bin"
        cp "$binary" "$bin"
      done

      wrapProgram "$bin" \
        --set-default INSTA_WORKSPACE_ROOT "${./..}"
    '';
  })
