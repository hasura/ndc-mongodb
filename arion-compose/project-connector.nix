# Defines a docker-compose project that runs the full set of services to run
# a GraphQL Engine instance and two MongoDB connectors. Matches the environment
# used for integration tests. This project is intended for interactive testing,
# so it maps host ports, and sets up a persistent volume for MongoDB.
#
# To see the service port numbers look at integration-test-services.nix
#
# To start this project run:
#
#     arion up -d
#

{ pkgs, ... }:
let
  services = import ./integration-test-services.nix {
    inherit pkgs mongodb-volume;
    map-host-ports = true;
    otlp-endpoint = "http://jaeger:4317";
  };

  mongodb-volume = "mongodb";
in
{
  project.name = "mongodb-connector";

  docker-compose.volumes = {
    ${mongodb-volume} = null;
  };

  services = services // {
    jaeger = import ./service-jaeger.nix { inherit pkgs; };
  };
}
