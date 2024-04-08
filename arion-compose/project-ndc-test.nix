{ pkgs, config, ... }:

let
  mongodb-port = "27017";
in
{
  project.name = "mongodb-ndc-test";

  services = {
    test = import ./service-connector.nix {
      inherit pkgs;
      command = ["test" "--snapshots-dir" "/snapshots" "--seed" "1337_1337_1337_1337_1337_1337_13"];
      # command = ["replay" "--snapshots-dir" "/snapshots"];
      configuration-dir = ../fixtures/connector/chinook;
      database-uri = "mongodb://mongodb:${mongodb-port}/chinook";
      service.depends_on.mongodb.condition = "service_healthy";
      # Run the container as the current user so when it writes to the snapshots directory it doesn't write as root
      service.user = builtins.toString config.host.uid;
      extra-volumes = [
        "./snapshots:/snapshots:rw"
      ];
    };

    mongodb = import ./service-mongodb.nix {
      inherit pkgs;
      port = mongodb-port;
      volumes = [
        (import ./fixtures-mongodb.nix).chinook
      ];
    };
  };
}
