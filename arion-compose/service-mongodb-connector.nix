# Provides an arion-compose service. Use in arion-compose.nix like this:
#
#     services = {
#       connector = import ./arion-compose/mongodb-connector.nix {
#         inherit pkgs;
#         port = "8100";
#       };
#     };
#
{ pkgs
, port ? "7130"
, profile ? "dev" # Rust crate profile, usually either "dev" or "release"
, hostPort ? null
, command ? "serve"
, configuration-dir ? ../fixtures/connector/chinook
, database-uri ? "mongodb://mongodb/chinook"
, service ? { } # additional options to customize this service configuration
, otlp-endpoint ? null
}:

let
  connector-pkg = pkgs.pkgsCross.linux.mongodb-connector.override { inherit profile; };

  connector-service = {
    useHostStore = true;
    command = [
      # mongodb-connector is added to pkgs via an overlay in flake.nix
      "${connector-pkg}/bin/mongodb-connector"
      command
    ];
    ports = pkgs.lib.optionals (hostPort != null) [
      "${hostPort}:${port}" # host:container
    ];
    environment = pkgs.lib.filterAttrs (_: v: v != null) {
      HASURA_CONFIGURATION_DIRECTORY = "/configuration";
      HASURA_CONNECTOR_PORT = port;
      MONGODB_DATABASE_URI = database-uri;
      OTEL_SERVICE_NAME = "mongodb-connector";
      OTEL_EXPORTER_OTLP_ENDPOINT = otlp-endpoint;
      RUST_LOG = "mongodb-connector=debug,dc_api=debug";
    };
    volumes = [
      "${configuration-dir}:/configuration:ro"
    ];
    healthcheck = {
      test = [ "CMD" "${pkgs.pkgsCross.linux.curl}/bin/curl" "-f" "http://localhost:${port}/health" ];
      start_period = "5s";
      interval = "5s";
      timeout = "1s";
      retries = 3;
    };
  };
in
{
  service =
    # merge service definition with overrides
    pkgs.lib.attrsets.recursiveUpdate connector-service service;
}
