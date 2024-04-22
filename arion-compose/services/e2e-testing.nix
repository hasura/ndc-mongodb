{ pkgs
, engine-graphql-url ? null
, service ? { } # additional options to customize this service configuration
}:

let
  v3-e2e-testing-source = builtins.fetchGit {
    url = "git+ssh://git@github.com/hasura/v3-e2e-testing?ref=jesse/update-mongodb";
    name = "v3-e2e-testing-source";
    ref = "jesse/update-mongodb";
    rev = "325240c938c253a21f2fe54161b0c94e54f1a3a5";
  };

  v3-e2e-testing = pkgs.pkgsCross.linux.callPackage ../../nix/v3-e2e-testing.nix { src = v3-e2e-testing-source; database-to-test = "mongodb"; };

  e2e-testing-service = {
    useHostStore = true;
    command = [
      "${v3-e2e-testing}/bin/v3-e2e-testing-mongodb"
    ];
    environment = pkgs.lib.optionalAttrs (engine-graphql-url != null) {
      ENGINE_GRAPHQL_URL = engine-graphql-url;
    };
  };
in
{
  service =
    # merge service definition with overrides
    pkgs.lib.attrsets.recursiveUpdate e2e-testing-service service;
}
