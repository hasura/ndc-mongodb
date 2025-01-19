# Create a standalone executable that runs integration tests when it is
# executed.

{ ndc-mongodb-workspace
, defaultCrateOverrides
, makeWrapper
}:

ndc-mongodb-workspace.workspaceMembers.integration-tests.build.override {
  features = [ "integration" ];
  crateOverrides = defaultCrateOverrides // {
    integration-tests = attrs: {
      extraRustcOpts = [ "--test" ]; # builds a test binary in target/lib/

      # Add programs we need for postInstall hook to nativeBuildInputs
      nativeBuildInputs = [
        makeWrapper
      ];

      # The test binary is not automatically installed to the output nix store
      # path so we do that here.
      postInstall = ''
        local bin="$out/bin/integration-tests"

        for binary in target/lib/*; do
          echo "installing '$binary' to '$bin'"
          mkdir -p "$out/bin"
          cp "$binary" "$bin"
        done

        # Set environment variable to point to source workspace so that `insta`
        # (the Rust snapshot test library) can find snapshot files.
        wrapProgram "$bin" \
          --set-default INSTA_WORKSPACE_ROOT "${./..}"
      '';
    };
  };
}
