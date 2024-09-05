{ pkgs, config, ... }:

let
  mongodb-port = "27017";
in
{
  project.name = "mongodb-ndc-test";

  services = {
    test = import ./services/connector.nix {
      inherit pkgs;
      command = ["test"];
      # Record snapshots into the snapshots dir
      # command = ["test" "--snapshots-dir" "/snapshots" "--seed" "1337_1337_1337_1337_1337_1337_13"];
      # Replay and test the recorded snapshots
      # command = ["replay" "--snapshots-dir" "/snapshots"];
      configuration-dir = ../fixtures/hasura/chinook/connector;
      database-uri = "mongodb://mongodb:${mongodb-port}/chinook";
      service.depends_on.mongodb.condition = "service_healthy";
      # Run the container as the current user so when it writes to the snapshots directory it doesn't write as root
      service.user = builtins.toString config.host.uid;
      extra-volumes = [
        # Mount the snapshots directory in the repo source tree into the container
        # so that ndc-test can read/write in it
        "./snapshots:/snapshots:rw"
      ];
    };

    mongodb = import ./services/mongodb.nix {
      inherit pkgs;
      port = mongodb-port;
      volumes = [
        (import ./fixtures/mongodb.nix).chinook
      ];
    };
  };
}
