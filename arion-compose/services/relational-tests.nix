{ pkgs
, mongodb-uri
, service ? { }
}:

let
  relational-tests-service = {
    useHostStore = true;
    command = [
      "${pkgs.pkgsCross.linux.relationalTests}/bin/relational-tests"
      "--ignored"
    ];
    environment = {
      MONGODB_URI = mongodb-uri;
    };
  };
in
{
  service =
    pkgs.lib.attrsets.recursiveUpdate relational-tests-service service;
}
