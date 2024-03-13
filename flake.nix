{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default";

    crane = {
      url = "github:ipetkov/crane";
      inputs.nixpkgs.follows = "nixpkgs";
    };

    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };

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

    # This gets the source for the v3-engine. We use an expression in
    # ./nix/v3-engine.nix to build. This is used to produce an arion service.
    #
    # To test against local engine changes, change the url here to:
    # 
    #     url = "git+file:///home/me/path/to/v3-engine"
    #
    # If source changes aren't picked up automatically try:
    #
    # - committing changes to the local engine repo
    # - running `nix flake lock --update-input v3-engine-source` in this repo
    # - arion up -d engine
    #
    v3-engine-source = {
      url = "git+ssh://git@github.com/hasura/v3-engine";
      flake = false;
    };

    # See the note above on v3-engine-source for information on running against
    # a version of v3-e2e-testing with local changes.
    v3-e2e-testing-source = {
      url = "git+ssh://git@github.com/hasura/v3-e2e-testing?ref=jesse/update-mongodb";
      flake = false;
    };
  };

  outputs =
    { self
    , nixpkgs
    , crane
    , rust-overlay
    , advisory-db
    , arion
    , v3-engine-source
    , v3-e2e-testing-source
    , systems
    , ...
    }:
    let
      # Nixpkgs provides a wide set of software packages. These overlays add
      # packages or replace packages in that set.
      overlays = [
        (import rust-overlay)
        (final: prev: rec {
          # What's the deal with `pkgsBuildHost`? It has to do with
          # cross-compiling.
          #
          # - "build" is the system we are building on
          # - "host" is the system we are building for
          #
          # If a package set is configured  for cross-compiling then packages in
          # the set by default are compiled to run on the "host" system. OTOH
          # `pkgsBuildHost` contains copies of all packages compiled to run on
          # the build system, and to produce outputs for the host system.
          rustToolchain = final.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
          craneLib = (crane.mkLib final).overrideToolchain rustToolchain;

          # Extend our package set with mongodb-connector, v3-engine, and other
          # packages built by this flake to make these packages accessible in
          # arion-compose.nix.
          mongodb-connector-workspace = final.callPackage ./nix/mongodb-connector-workspace.nix { }; # builds all packages in this repo
          mongodb-connector = final.mongodb-connector-workspace.override { package = "mongodb-connector"; }; # override `package` to build one specific crate
          mongodb-cli-plugin = final.mongodb-connector-workspace.override { package = "mongodb-cli-plugin"; };
          v3-engine = final.callPackage ./nix/v3-engine.nix { src = v3-engine-source; };
          v3-e2e-testing = final.callPackage ./nix/v3-e2e-testing.nix { src = v3-e2e-testing-source; database-to-test = "mongodb"; };
          inherit v3-e2e-testing-source; # include this source so we can read files from it in arion-compose configs
          dev-auth-webhook = final.callPackage ./nix/dev-auth-webhook.nix { src = "${v3-engine-source}/hasura-authn-webhook/dev-auth-webhook"; };

          # Provide cross-compiled versions of each of our packages under
          # `pkgs.pkgsCross.${system}.${package-name}`
          pkgsCross.aarch64-linux = mkPkgsCross final.buildPlatform.system "aarch64-linux";
          pkgsCross.x86_64-linux = mkPkgsCross final.buildPlatform.system "x86_64-linux";

          # Provide cross-compiled versions of each of our packages that are
          # compiled for Linux but with the same architecture as `localSystem`.
          # This is useful for building Docker images on Mac developer machines.
          pkgsCross.linux = mkPkgsLinux final.buildPlatform.system;
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

      # Like the above, but build for Linux while matching the architecture we
      # are building on.
      mkPkgsLinux = localSystem: import nixpkgs {
        inherit localSystem overlays;
        crossSystem = "${(mkPkgs localSystem).stdenv.buildPlatform.qemuArch}-linux";
      };

      # Helper to define flake outputs for multiple systems.
      eachSystem = callback: nixpkgs.lib.genAttrs (import systems) (system: callback (mkPkgs system));

    in
    {
      checks = eachSystem (pkgs: {
        # Build all crates as part of `nix flake check`
        inherit (pkgs) mongodb-connector-workspace;

        lint = pkgs.craneLib.cargoClippy (pkgs.mongodb-connector-workspace.buildArgs // {
          cargoClippyExtraArgs = "--all-targets -- --deny warnings";
          doInstallCargoArtifacts = false; # avoids "wrong ELF type" messages
        });

        test = pkgs.craneLib.cargoNextest (pkgs.mongodb-connector-workspace.buildArgs // {
          partitions = 1;
          partitionType = "count";
          doInstallCargoArtifacts = false; # avoids "wrong ELF type" messages
        });

        audit = pkgs.craneLib.cargoAudit {
          inherit advisory-db;
          inherit (pkgs.mongodb-connector-workspace) src;
        };
      });

      packages = eachSystem (pkgs: {
        default = pkgs.mongodb-connector;

        # Note: these outputs are overridden to build statically-linked
        mongodb-connector-x86_64-linux = pkgs.pkgsCross.x86_64-linux.mongodb-connector.override { staticallyLinked = true; };
        mongodb-connector-aarch64-linux = pkgs.pkgsCross.aarch64-linux.mongodb-connector.override { staticallyLinked = true; };

        docker = pkgs.callPackage ./nix/docker.nix { inherit (pkgs) mongodb-connector; };

        docker-x86_64-linux = pkgs.callPackage ./nix/docker.nix {
          mongodb-connector = pkgs.pkgsCross.x86_64-linux.mongodb-connector; # Note: dynamically-linked
          architecture = "amd64";
        };

        docker-aarch64-linux = pkgs.callPackage ./nix/docker.nix {
          mongodb-connector = pkgs.pkgsCross.aarch64-linux.mongodb-connector; # Note: dynamically-linked
          architecture = "arm64";
        };

        # CLI plugin packages with cross-compilation options
        mongodb-cli-plugin = pkgs.mongodb-cli-plugin.override { staticallyLinked = true; };
        mongodb-cli-plugin-x86_64-linux = pkgs.pkgsCross.x86_64-linux.mongodb-cli-plugin.override { staticallyLinked = true; };
        mongodb-cli-plugin-aarch64-linux = pkgs.pkgsCross.aarch64-linux.mongodb-cli-plugin.override { staticallyLinked = true; };

        # CLI plugin docker images
        mongodb-cli-plugin-docker = pkgs.callPackage ./nix/docker-cli-plugin.nix { };
        mongodb-cli-plugin-docker-x86_64-linux = pkgs.pkgsCross.x86_64-linux.callPackage ./nix/docker-cli-plugin.nix { };
        mongodb-cli-plugin-docker-aarch64-linux = pkgs.pkgsCross.aarch64-linux.callPackage ./nix/docker-cli-plugin.nix { };

        publish-docker-image = pkgs.writeShellApplication {
          name = "publish-docker-image";
          runtimeInputs = with pkgs; [ coreutils skopeo ];
          text = builtins.readFile ./deploy.sh;
        };
      });

      # Export our nixpkgs package set, which has been extended with the
      # mongodb-connector, v3-engine, etc. We do this so that arion can pull in
      # the same package set through arion-pkgs.nix.
      legacyPackages = eachSystem (pkgs: pkgs);

      devShells = eachSystem (pkgs: {
        default = pkgs.mkShell {
          inputsFrom = builtins.attrValues self.checks.${pkgs.buildPlatform.system};
          nativeBuildInputs = with pkgs; [
            arion.packages.${pkgs.buildPlatform.system}.default
            just
            mongosh
            pkg-config
          ];
        };
      });
    };
}
