{ pkgs, ... }:

let
  connector-port = "7130";
  engine-port = "7100";
  mongodb-port = "27017";
in
{
  project.name = "mongodb-e2e-testing";

  services = {
    test = import ./service-e2e-testing.nix {
      inherit pkgs;
      engine-graphql-url = "http://engine:${engine-port}/graphql";
      service.depends_on = {
        connector.condition = "service_healthy";
        engine.condition = "service_healthy";
      };
    };

    connector = import ./service-mongodb-connector.nix {
      inherit pkgs;
      port = connector-port;
      service.depends_on.mongodb.condition = "service_healthy";
    };

    mongodb = import ./service-mongodb.nix {
      inherit pkgs;
      port = mongodb-port;
      volumes = [
        (import ./fixtures-mongodb.nix).chinook
      ];
    };

    engine = import ./service-engine.nix {
      inherit pkgs;
      port = engine-port;
      connector-url = "http://connector:${connector-port}";
      service.depends_on = {
        auth-hook.condition = "service_started";
      };
    };

    auth-hook = import ./service-dev-auth-webhook.nix { inherit pkgs; };
  };
}
