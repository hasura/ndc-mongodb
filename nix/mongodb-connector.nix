{ callPackage, ... }@args:
callPackage ./mongodb-connector-workspace.nix (args // {
  package = "mongodb-connector";
})
