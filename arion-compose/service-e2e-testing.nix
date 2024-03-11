{ pkgs
, engine-graphql-url ? null
, service ? { } # additional options to customize this service configuration
}:

let
  e2e-testing-service = {
    useHostStore = true;
    command = [
      "${pkgs.pkgsCross.linux.v3-e2e-testing}/bin/v3-e2e-testing-mongodb"
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
