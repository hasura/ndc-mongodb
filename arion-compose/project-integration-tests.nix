{ pkgs, ... }:
let
  connector-port = "7230";
  connector-chinook-port = "7231";
  engine-port = "7200";
  mongodb-port = "27017";
in
{
  project.name = "mongodb-connector-integration-tests";

  services = {
    # Uncomment to run tests in docker without requiring host port mapping. This
    # has the disadvantage that the integration test framework cannot write
    # updated snapshots.
    #
    # test = import ./service-integration-tests.nix {
    #   inherit pkgs;
    #   engine-graphql-url = "http://engine:${engine-port}/graphql";
    #   service.depends_on = {
    #     connector.condition = "service_healthy";
    #     connector-chinook.condition = "service_healthy";
    #     engine.condition = "service_healthy";
    #   };
    # };

    connector = import ./service-connector.nix {
      inherit pkgs;
      configuration-dir = ../fixtures/connector/sample_mflix;
      database-uri = "mongodb://mongodb/sample_mflix";
      port = connector-port;
      hostPort = connector-port;
      service.depends_on = {
        mongodb.condition = "service_healthy";
      };
    };

    connector-chinook = import ./service-connector.nix {
      inherit pkgs;
      configuration-dir = ../fixtures/connector/chinook;
      database-uri = "mongodb://mongodb/chinook";
      port = connector-chinook-port;
      hostPort = connector-chinook-port;
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
      service.depends_on = {
        auth-hook.condition = "service_started";
      };
    };

    auth-hook = import ./service-dev-auth-webhook.nix { inherit pkgs; };
  };
}

