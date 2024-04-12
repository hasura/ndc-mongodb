{ pkgs, ... }:

let
  mongodb-port = "27017";
in
{
  project.name = "mongodb-ndc-test";

  services = {
    test = import ./service-mongodb-connector.nix {
      inherit pkgs;
      command = "test";
      database-uri = "mongodb://mongodb:${mongodb-port}/chinook";
    };

    mongodb = import ./service-mongodb.nix {
      inherit pkgs;
      port = mongodb-port;
    };
  };
}
