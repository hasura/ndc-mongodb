{ pkgs, config, ... }:
{
  project.name = "mongodb-connector-relational-tests";

  services = {
    mongodb = import ./services/mongodb.nix {
      inherit pkgs;
      mongodb-image = "mongo:7";
    };

    test = import ./services/relational-tests.nix {
      inherit pkgs;
      mongodb-uri = "mongodb://mongodb/test_relational";
      service.depends_on = {
        mongodb.condition = "service_healthy";
      };
      service.user = builtins.toString config.host.uid;
    };
  };
}
