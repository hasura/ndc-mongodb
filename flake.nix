{
  inputs = {
    # nixpkgs provides packages such as mongosh and just, and provides libraries
    # used to build the connector like openssl
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";

    # Nix build system for Rust projects. Builds each crate (including
    # dependencies) as a separate nix derivation for best possible cache
    # utilization.
    # crate2nix.url = "github:nix-community/crate2nix";
    crate2nix.url = "github:hallettj/crate2nix/git-workspaces";

    hasura-ddn-cli.url = "github:hasura/ddn-cli-nix";

    # Allows selecting arbitrary Rust toolchain configurations by editing
    # `rust-toolchain.toml`
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # Security audit data for Rust projects
    advisory-db = {
      url = "github:rustsec/advisory-db";
      flake = false;
    };

    # Nix interface to docker-compose
    arion = {
      url = "github:hercules-ci/arion";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    # We need flake-compat in arion-pkgs.nix
    flake-compat.url = "github:edolstra/flake-compat";

    # This gets the source for the graphql engine. We use an expression in
    # ./nix/graphql-engine.nix to build. This is used to produce an arion
    # service.
    #
    # To test against local engine changes, change the url here to:
    # 
    #     url = "git+file:///home/me/path/to/graphql-engine"
    #
    # If source changes aren't picked up automatically try:
    #
    # - committing changes to the local engine repo
    # - running `nix flake update graphql-engine-source` in this repo
    # - arion up -d engine
    #
    graphql-engine-source = {
      url = "github:hasura/graphql-engine";
      flake = false;
    };
  };

  outputs =
    { self
    , nixpkgs
    , crate2nix
    , hasura-ddn-cli
    , rust-overlay
    , advisory-db
    , arion
    , graphql-engine-source
    , systems
    , ...
    }:
    let
      # Nixpkgs provides a wide set of software packages. These overlays add
      # packages or replace packages in that set.
      overlays = [
        (import rust-overlay)
        (final: prev: {
          rustToolchain = final.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;

          # Cargo.nix is a generated set of nix derivations that builds workspace
          # crates. We import it here to bring project crates into the overlayed
          # package set.
          #
          # To apply the Rust toolchain described in `rust-toolchain.toml` we need
          # to override `cargo` and `rustc` inputs. Note that `rustToolchain`
          # is set up above.
          ndc-mongodb-workspace = import ./Cargo.nix {
            pkgs = final;
            buildRustCrateForPkgs = pkgs: pkgs.buildRustCrate.override {
              # What's the deal with `pkgsBuildHost`? It has to do with
              # cross-compiling.
              #
              # - "build" is the system we are building on
              # - "host" is the system we are building for
              #
              # If a package set is configured for cross-compiling then packages
              # in the set by default are compiled to run on the "host" system.
              # OTOH `pkgsBuildHost` contains copies of all packages compiled to
              # run on the build system, and to produce compiled output for the
              # host system.
              #
              # So it's important to use packages in `pkgsBuildHost` to
              # reference programs that run during the build process.
              cargo = pkgs.pkgsBuildHost.rustToolchain;
              rustc = pkgs.pkgsBuildHost.rustToolchain;
            };
          };

          # Extend our package set with mongodb-connector, graphql-engine, and
          # other packages built by this flake to make these packages accessible
          # in arion-compose.nix.
          mongodb-connector = final.ndc-mongodb-workspace.workspaceMembers.mongodb-connector.build;
          mongodb-cli-plugin = final.ndc-mongodb-workspace.workspaceMembers.mongodb-cli-plugin.build;

          # Packages created by crate2nix like these can be customized in
          # a number of ways. For example to create a debug build with
          # extra features enabled use an expression like this:
          #
          #     final.ndc-mongodb-workspace.workspaceMembers.mongodb-connector.build.override {
          #       features = [ "default" "optional-feature" "etc" ];
          #       release = false;
          #     }
          #

          graphql-engine-workspace =
            let
              src = "${graphql-engine-source}/v3";
              rust = final.pkgsBuildHost.rust-bin.fromRustupToolchainFile "${src}/rust-toolchain.toml";
              cargo-nix = crate2nix.tools.${final.system}.generatedCargoNix {
                name = "graphql-engine-workspace";
                inherit src;
              };
            in
            import cargo-nix {
              pkgs = final;
              buildRustCrateForPkgs = pkgs: pkgs.buildRustCrate.override {
                cargo = rust;
                rustc = rust;
              };
            };

          graphql-engine = final.graphql-engine-workspace.workspaceMembers.engine.build;
          dev-auth-webhook = final.graphql-engine-workspace.workspaceMembers.dev-auth-webhook;

          integration-tests = final.callPackage ./nix/integration-tests.nix { };

          # Provide cross-compiled versions of each of our packages under
          # `pkgs.pkgsCross.${system}.${package-name}`
          pkgsCross.aarch64-linux = mkPkgsCross final.buildPlatform.system "aarch64-linux";
          pkgsCross.x86_64-linux = mkPkgsCross final.buildPlatform.system "x86_64-linux";

          # Provide cross-compiled versions of each of our packages that are
          # compiled for Linux but with the same architecture as `localSystem`.
          # This is useful for building Docker images on Mac developer machines.
          pkgsCross.linux = mkPkgsLinux final.buildPlatform.system;

          ddn = hasura-ddn-cli.packages.${final.system}.default;
        })
      ];

      # Our default package set is configured to build for the same platform
      # the flake is evaluated on. So we leave `crossSystem` set to the default,
      # which is `crossSystem = localSystem`. With this package set if we're
      # building on Linux we get Linux binaries, if we're building on Mac we get
      # Mac binaries, etc.
      mkPkgs = localSystem: import nixpkgs { inherit localSystem overlays; };

      # In a package set with a `crossSystem` that is different from
      # `localSystem` packages are implicitly cross-compiled to run on
      # `crossSystem`.
      mkPkgsCross = localSystem: crossSystem: import nixpkgs { inherit localSystem crossSystem overlays; };

      # Like mkPkgsCross, but build for Linux while matching the architecture we
      # are building on.
      mkPkgsLinux = localSystem: import nixpkgs {
        inherit localSystem overlays;
        crossSystem = "${(mkPkgs localSystem).stdenv.buildPlatform.qemuArch}-linux";
      };

      # Helper to define flake outputs for multiple systems.
      eachSystem = callback: nixpkgs.lib.genAttrs (import systems) (system: callback (mkPkgs system));

    in
    {
      # checks = eachSystem (pkgs: {
      #   # Build all crates as part of `nix flake check`
      #   inherit (pkgs) mongodb-connector-workspace;
      #
      #   lint = pkgs.craneLib.cargoClippy (pkgs.mongodb-connector-workspace.buildArgs // {
      #     cargoClippyExtraArgs = "--all-targets -- --deny warnings";
      #     doInstallCargoArtifacts = false; # avoids "wrong ELF type" messages
      #   });
      #
      #   test = pkgs.craneLib.cargoNextest (pkgs.mongodb-connector-workspace.buildArgs // {
      #     partitions = 1;
      #     partitionType = "count";
      #     doInstallCargoArtifacts = false; # avoids "wrong ELF type" messages
      #   });
      #
      #   audit = pkgs.craneLib.cargoAudit {
      #     inherit advisory-db;
      #     inherit (pkgs.mongodb-connector-workspace) src;
      #   };
      # });

      packages = eachSystem (pkgs: rec {
        default = pkgs.mongodb-connector;

        # Some of the package definitions below are cross-compiled,
        # statically-linked, or both. Here's how that works:
        #
        # - everything in `pkgsCross.${system}` is automatically cross-compiled for the named system
        # - everything in `pkgsStatic` is statically-linked (note that static linking only works for linux builds)
        #
        # Those can be combined, but `pkgsCross` must be specified first. For
        # example to get a cross-compiled, statically-linked build of the
        # connector for ARM linux:
        #
        #     pkgs.pkgsCross.aarch64-linux.pkgsStatic.mongodb-connector
        #
        # If there is a build you want that is not listed here you can reference
        # `pkgs` via the `legacyPackages` flake output. For example to get
        # a connector build for ARM Linux that is **not** statically linked run,
        #
        #     $ nix build .#pkgsCross.aarch64-linux.mongodb-connector
        #

        # Note: these outputs are overridden to build statically-linked
        mongodb-connector-x86_64-linux = pkgs.pkgsCross.x86_64-linux.pkgsStatic.mongodb-connector;
        mongodb-connector-aarch64-linux = pkgs.pkgsCross.aarch64-linux.pkgsStatic.mongodb-connector;

        # Builds a docker image for the MongoDB connector for amd64 Linux. To
        # get a multi-arch image run `publish-docker-image`.
        docker-image-x86_64-linux = pkgs.pkgsCross.x86_64-linux.callPackage ./nix/docker-connector.nix { };

        # Builds a docker image for the MongoDB connector for arm64 Linux. To
        # get a multi-arch image run `publish-docker-image`.
        docker-image-aarch64-linux = pkgs.pkgsCross.aarch64-linux.callPackage ./nix/docker-connector.nix { };

        # Publish multi-arch docker image for the MongoDB connector to Github
        # registry. This must be run with a get-ref argument to calculate image
        # tags:
        #
        #     $ nix run .#publish-docker-image <git-ref>
        #
        # You must be logged in to the docker registry. See the CI configuration
        # in `.github/workflows/deploy.yml` where this command is run.
        publish-docker-image = pkgs.callPackage ./scripts/publish-docker-image.nix {
          docker-images = [
            docker-image-aarch64-linux
            docker-image-x86_64-linux
          ];
        };

        # CLI plugin packages with cross-compilation options
        mongodb-cli-plugin = pkgs.mongodb-cli-plugin;
        mongodb-cli-plugin-x86_64-linux = pkgs.pkgsCross.x86_64-linux.mongodb-cli-plugin.override { staticallyLinked = true; };
        mongodb-cli-plugin-aarch64-linux = pkgs.pkgsCross.aarch64-linux.mongodb-cli-plugin.override { staticallyLinked = true; };

        # CLI plugin docker images
        mongodb-cli-plugin-docker = pkgs.callPackage ./nix/docker-cli-plugin.nix { };
        mongodb-cli-plugin-docker-x86_64-linux = pkgs.pkgsCross.x86_64-linux.callPackage ./nix/docker-cli-plugin.nix { };
        mongodb-cli-plugin-docker-aarch64-linux = pkgs.pkgsCross.aarch64-linux.callPackage ./nix/docker-cli-plugin.nix { };
      });

      # Export our nixpkgs package set, which has been extended with the
      # mongodb-connector, graphql-engine, etc. We do this so that arion can pull in
      # the same package set through arion-pkgs.nix.
      legacyPackages = eachSystem (pkgs: pkgs);

      devShells = eachSystem (pkgs: {
        default = pkgs.mkShell {
          # inputsFrom = builtins.attrValues self.checks.${pkgs.buildPlatform.system};
          nativeBuildInputs = with pkgs; [
            arion.packages.${pkgs.system}.default
            cargo-insta
            crate2nix.packages.${pkgs.system}.default
            ddn
            just
            mongosh
            pkg-config
          ] ++ nixpkgs.lib.optionals pkgs.stdenv.isDarwin (with pkgs; [
            libiconv
          ]);
        };
      });
    };
}
