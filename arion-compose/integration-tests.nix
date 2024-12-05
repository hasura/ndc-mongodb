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
  connector-port = "7130";
  connector-chinook-port = "7131";
  connector-test-cases-port = "7132";
  engine-port = "3280";

  services = import ./integration-test-services.nix {
    inherit pkgs connector-port connector-chinook-port engine-port;
    map-host-ports = false;
  };
in
{
  project.name = "mongodb-connector-integration-tests";

  services = services // {
    test = import ./services/integration-tests.nix {
      inherit pkgs;
      connector-url = "http://connector:${connector-port}/";
      connector-chinook-url = "http://connector-chinook:${connector-chinook-port}/";
      connector-test-cases-url = "http://connector-test-cases:${connector-test-cases-port}/";
      engine-graphql-url = "http://engine:${engine-port}/graphql";
      service.depends_on = {
        connector.condition = "service_healthy";
        connector-chinook.condition = "service_healthy";
        connector-test-cases.condition = "service_healthy";
        engine.condition = "service_healthy";
      };
      # Run the container as the current user so when it writes to the snapshots
      # directory it doesn't write as root
      service.user = builtins.toString config.host.uid;
    };
  };
}
