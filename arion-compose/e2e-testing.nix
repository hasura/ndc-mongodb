{ pkgs, ... }:

let
  connector-port = "7130";
  engine-port = "7100";
  mongodb-port = "27017";
in
{
  project.name = "mongodb-e2e-testing";

  services = {
    test = import ./services/e2e-testing.nix {
      inherit pkgs;
      engine-graphql-url = "http://engine:${engine-port}/graphql";
      service.depends_on = {
        connector.condition = "service_healthy";
        engine.condition = "service_healthy";
      };
    };

    connector = import ./services/connector.nix {
      inherit pkgs;
      configuration-dir = ../fixtures/connector/chinook;
      database-uri = "mongodb://mongodb/chinook";
      port = connector-port;
      service.depends_on.mongodb.condition = "service_healthy";
    };

    mongodb = import ./services/mongodb.nix {
      inherit pkgs;
      port = mongodb-port;
      volumes = [
        (import ./fixtures/mongodb.nix).chinook
      ];
    };

    engine = import ./services/engine.nix {
      inherit pkgs;
      port = engine-port;
      connectors.chinook = "http://connector:${connector-port}";
      ddn-dirs = [ ../fixtures/ddn/chinook ];
      service.depends_on = {
        auth-hook.condition = "service_started";
      };
    };

    auth-hook = import ./services/dev-auth-webhook.nix { inherit pkgs; };
  };
}
