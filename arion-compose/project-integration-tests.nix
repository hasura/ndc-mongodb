# Defines a docker-compose project that runs the full set of services to run
# a GraphQL Engine instance and two MongoDB connectors, and runs integration
# tests using those services.
# 
# To start this project run:
#
#     arion -f arion-compose/project-integration-tests.nix up -d
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
    test = import ./service-integration-tests.nix {
      inherit pkgs;
      engine-graphql-url = "http://engine:${engine-port}/graphql";
      service.depends_on = {
        connector.condition = "service_healthy";
        connector-chinook.condition = "service_healthy";
        engine.condition = "service_healthy";
      };
      service.user = builtins.toString config.host.uid;
    };
  };
}
