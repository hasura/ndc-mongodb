# Run 2 MongoDB connectors and engine with supporting database. Running two
# connectors is useful for testing remote joins.
#
# To start this # project run:
#
#     arion -f arion-compose/project-connector.nix up -d
#

{ pkgs, ... }:
let
  connector-port = "7130";
  connector-chinook-port = "7131";
  engine-port = "7100";
  mongodb-port = "27017";
in
{
  project.name = "mongodb-connector";

  services = {
    connector = import ./service-connector.nix {
      inherit pkgs;
      configuration-dir = ../fixtures/connector/sample_mflix;
      database-uri = "mongodb://mongodb/sample_mflix";
      port = connector-port;
      hostPort = connector-port;
      otlp-endpoint = "http://jaeger:4317";
      service.depends_on = {
        jaeger.condition = "service_healthy";
        mongodb.condition = "service_healthy";
      };
    };

    connector-chinook = import ./service-connector.nix {
      inherit pkgs;
      configuration-dir = ../fixtures/connector/chinook;
      database-uri = "mongodb://mongodb/chinook";
      port = connector-chinook-port;
      hostPort = connector-chinook-port;
      otlp-endpoint = "http://jaeger:4317";
      service.depends_on = {
        jaeger.condition = "service_healthy";
        mongodb.condition = "service_healthy";
      };
    };

    mongodb = import ./service-mongodb.nix {
      inherit pkgs;
      port = mongodb-port;
      hostPort = mongodb-port;
      volumes = [
        "mongodb:/data/db"
        (import ./fixtures-mongodb.nix).all-fixtures
      ];
    };

    engine = import ./service-engine.nix {
      inherit pkgs;
      port = engine-port;
      hostPort = engine-port;
      auth-webhook = { url = "http://auth-hook:3050/validate-request"; };
      connectors = {
        chinook = "http://connector-chinook:${connector-chinook-port}";
        sample_mflix = "http://connector:${connector-port}";
      };
      ddn-dirs = [
        ../fixtures/ddn/chinook
        ../fixtures/ddn/sample_mflix
        ../fixtures/ddn/remote-relationships_chinook-sample_mflix
      ];
      otlp-endpoint = "http://jaeger:4317";
      service.depends_on = {
        auth-hook.condition = "service_started";
        jaeger.condition = "service_healthy";
      };
    };

    auth-hook = import ./service-dev-auth-webhook.nix { inherit pkgs; };

    jaeger = import ./service-jaeger.nix { inherit pkgs; };
  };

  docker-compose.volumes = {
    mongodb = null;
  };
}

