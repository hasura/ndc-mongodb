{ pkgs
, engine-graphql-url
, service ? { } # additional options to customize this service configuration
}:

let
  integration-tests-service = {
    useHostStore = true;
    command = [
      "${pkgs.pkgsCross.linux.integration-tests}/bin/integration-tests"
    ];
    environment = {
      ENGINE_GRAPHQL_URL = engine-graphql-url;
    };
  };
in
{
  service =
    # merge service definition with overrides
    pkgs.lib.attrsets.recursiveUpdate integration-tests-service service;
}
