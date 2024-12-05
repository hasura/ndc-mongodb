# Run 2 MongoDB connectors and engine with supporting database. Running two
# connectors is useful for testing remote joins.
#
# This expression defines a set of docker-compose services, but does not specify
# a full docker-compose project by itself. It should be imported into a project
# definition. See arion-compose/default.nix and
# arion-compose/integration-tests.nix.

{ pkgs
, map-host-ports ? false
, mongodb-volume ? null
, otlp-endpoint ? null
, connector-port ? "7130"
, connector-chinook-port ? "7131"
, connector-test-cases-port ? "7132"
, engine-port ? "3280"
, mongodb-port ? "27017"
}:
let
  hostPort = port: if map-host-ports then port else null;
in
{
  connector = import ./services/connector.nix {
    inherit pkgs otlp-endpoint;
    configuration-dir = ../fixtures/hasura/app/connector/sample_mflix;
    database-uri = "mongodb://mongodb/sample_mflix";
    port = connector-port;
    hostPort = hostPort connector-port;
    service.depends_on = {
      mongodb.condition = "service_healthy";
    };
  };

  connector-chinook = import ./services/connector.nix {
    inherit pkgs otlp-endpoint;
    configuration-dir = ../fixtures/hasura/app/connector/chinook;
    database-uri = "mongodb://mongodb/chinook";
    port = connector-chinook-port;
    hostPort = hostPort connector-chinook-port;
    service.depends_on = {
      mongodb.condition = "service_healthy";
    };
  };

  connector-test-cases = import ./services/connector.nix {
    inherit pkgs otlp-endpoint;
    configuration-dir = ../fixtures/hasura/app/connector/test_cases;
    database-uri = "mongodb://mongodb/test_cases";
    port = connector-test-cases-port;
    hostPort = hostPort connector-test-cases-port;
    service.depends_on = {
      mongodb.condition = "service_healthy";
    };
  };

  mongodb = import ./services/mongodb.nix {
    inherit pkgs;
    port = mongodb-port;
    hostPort = hostPort mongodb-port;
    volumes = [
      (import ./fixtures/mongodb.nix).all-fixtures
    ] ++ pkgs.lib.optionals (mongodb-volume != null) [
      "${mongodb-volume}:/data/db"
    ];
  };

  engine = import ./services/engine.nix {
    inherit pkgs otlp-endpoint;
    port = engine-port;
    hostPort = hostPort engine-port;
    auth-webhook = { url = "http://auth-hook:3050/validate-request"; };
    connectors = {
      chinook = "http://connector-chinook:${connector-chinook-port}";
      sample_mflix = "http://connector:${connector-port}";
      test_cases = "http://connector-test-cases:${connector-test-cases-port}";
    };
    ddn-dirs = [
      ../fixtures/hasura/app/metadata
    ];
    service.depends_on = {
      auth-hook.condition = "service_started";
    };
  };

  auth-hook = import ./services/dev-auth-webhook.nix { inherit pkgs; };
}
