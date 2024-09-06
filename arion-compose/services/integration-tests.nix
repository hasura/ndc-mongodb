{ pkgs
, connector-url
, connector-chinook-url
, connector-test-cases-url
, engine-graphql-url
, service ? { } # additional options to customize this service configuration
}:

let
  repo-source-mount-point = "/src";

  integration-tests-service = {
    useHostStore = true;
    command = [
      "${pkgs.pkgsCross.linux.integration-tests}/bin/integration-tests"
    ];
    environment = {
      CONNECTOR_URL = connector-url;
      CONNECTOR_CHINOOK_URL = connector-chinook-url;
      CONNECTOR_TEST_CASES_URL = connector-test-cases-url;
      ENGINE_GRAPHQL_URL = engine-graphql-url;
      INSTA_WORKSPACE_ROOT = repo-source-mount-point;
      MONGODB_IMAGE = builtins.getEnv "MONGODB_IMAGE";
    };
    volumes = [
      "${builtins.getEnv "PWD"}:${repo-source-mount-point}:rw"
    ];
  };
in
{
  service =
    # merge service definition with overrides
    pkgs.lib.attrsets.recursiveUpdate integration-tests-service service;
}
