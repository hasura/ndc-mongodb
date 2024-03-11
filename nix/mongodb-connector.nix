# Override the `package` argument of the mongo-connector-workspace expression to
# build a specific binary.
{ callPackage, ... }@args:
callPackage ./mongodb-connector-workspace.nix (args // {
  package = "mongodb-connector";
})
