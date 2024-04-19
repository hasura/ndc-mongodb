{ pkgs, ... }:
let
  connector-port = "7130";
  connector-chinook-port = "7131";
  engine-port = "7100";
  mongodb-port = "27017";
in
{
  project.name = "mongodb-connector-integration-tests";

  services = {
    test = import ./service-integration-tests.nix {
      inherit pkgs;
      engine-graphql-url = "http://engine:${engine-port}/graphql";
      service.depends_on = {
        connector.condition = "service_healthy";
        connector-chinook.condition = "service_healthy";
        engine.condition = "service_healthy";
      };
    };

    connector = import ./service-connector.nix {
      inherit pkgs;
      configuration-dir = ../fixtures/connector/sample_mflix;
      database-uri = "mongodb://mongodb/sample_mflix";
      port = connector-port;
      service.depends_on = {
        mongodb.condition = "service_healthy";
      };
    };

    connector-chinook = import ./service-connector.nix {
      inherit pkgs;
      configuration-dir = ../fixtures/connector/chinook;
      database-uri = "mongodb://mongodb/chinook";
      port = connector-chinook-port;
      service.depends_on = {
        mongodb.condition = "service_healthy";
      };
    };

    mongodb = import ./service-mongodb.nix {
      inherit pkgs;
      port = mongodb-port;
      volumes = [
        (import ./fixtures-mongodb.nix).all-fixtures
      ];
    };

    engine = import ./service-engine.nix {
      inherit pkgs;
      port = engine-port;
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
      service.depends_on = {
        auth-hook.condition = "service_started";
      };
    };

    auth-hook = import ./service-dev-auth-webhook.nix { inherit pkgs; };
  };
}

