# Defines a docker-compose project that runs the full set of services to run
# a GraphQL Engine instance and two MongoDB connectors, and runs integration
# tests using those services.
# 
# To start this project run:
#
#     arion -f arion-compose/integration-tests.nix up -d
#

{ pkgs, config, ... }:
let
  services = import ./integration-test-services.nix {
    inherit pkgs engine-port;
    map-host-ports = false;
  };

  engine-port = "7100";
in
{
  project.name = "mongodb-connector-integration-tests";

  services = services // {
    test = import ./services/integration-tests.nix {
      inherit pkgs;
      engine-graphql-url = "http://engine:${engine-port}/graphql";
      service.depends_on = {
        connector.condition = "service_healthy";
        connector-chinook.condition = "service_healthy";
        engine.condition = "service_healthy";
      };
      # Run the container as the current user so when it writes to the snapshots
      # directory it doesn't write as root
      service.user = builtins.toString config.host.uid;
    };
  };
}
