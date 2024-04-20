# Provides an arion-compose service. Use in arion-compose.nix like this:
#
#     services = {
#       mongodb = import ./arion-compose/mongodb-service.nix {
#         inherit pkgs;
#         port = "27017"; 
#       };
#     };
#
{ pkgs
, port ? "27017"
, hostPort ? null
, environment ? {}
, volumes ? [
    # By default load fixtures in the mongo-connector repo
    (import ../fixtures/mongodb.nix).chinook
  ]
}:

let
  MONGO_INITDB_DATABASE = "test";
in
{
  service = {
    image = "mongo:6-jammy";
    environment = { inherit MONGO_INITDB_DATABASE; } // environment;
    inherit volumes;
    ports = pkgs.lib.optionals (hostPort != null) [ "${hostPort}:${port}" ];
    healthcheck = {
      test = [ "CMD-SHELL" ''echo 'db.runCommand("ping").ok' | mongosh localhost:27017/${MONGO_INITDB_DATABASE} --quiet'' ];
      interval = "5s";
      timeout = "10s";
      retries = 5;
      start_period = "10s";
    };
  };
}
