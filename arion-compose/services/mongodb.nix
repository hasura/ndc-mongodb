# Provides an arion-compose service. Use in arion-compose.nix like this:
#
#     services = {
#       mongodb = import ./arion-compose/services/mongodb.nix {
#         inherit pkgs;
#         port = "27017"; 
#       };
#     };
#
{ pkgs
, port ? "27017"
, hostPort ? null
, mongodb-image ? "mongo:7"
, environment ? { }
, volumes ? [
    # By default load fixtures in the mongo-connector repo
    (import ../fixtures/mongodb.nix).allFixtures
  ]
}:

let
  MONGO_INITDB_DATABASE = "test";

  image-from-env = builtins.getEnv "MONGODB_IMAGE";
  image = if image-from-env != "" then image-from-env else mongodb-image;

  # Prior to v6 MongoDB provides an older client shell called "mongo". The new
  # shell in v6 and later is called "mongosh"
  mongosh = if builtins.lessThan major-version 6 then "mongo" else "mongosh";
  major-version = pkgs.lib.toInt (builtins.head (builtins.match ".*:([0-9]).*" image));
in
{
  service = {
    inherit image volumes;
    environment = { inherit MONGO_INITDB_DATABASE; } // environment;
    ports = pkgs.lib.optionals (hostPort != null) [ "${hostPort}:${port}" ];
    healthcheck = {
      test = [ "CMD-SHELL" ''echo 'db.runCommand("ping").ok' | ${mongosh} localhost:${port}/${MONGO_INITDB_DATABASE} --quiet'' ];
      interval = "5s";
      timeout = "10s";
      retries = 5;
      start_period = "10s";
    };
  };
}
