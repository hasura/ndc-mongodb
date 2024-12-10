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
, command ? ["serve"]
, configuration-dir ? ../../fixtures/hasura/app/connector/sample_mflix
, database-uri ? "mongodb://mongodb/sample_mflix"
, service ? { } # additional options to customize this service configuration
, otlp-endpoint ? null
, extra-volumes ? [],
}:

let
  connector-pkg = pkgs.pkgsCross.linux.mongodb-connector.override { inherit profile; };

  connector-service = {
    useHostStore = true;
    command = [
      # mongodb-connector is added to pkgs via an overlay in flake.nix
      "${connector-pkg}/bin/mongodb-connector"
    ] ++ command;
    ports = pkgs.lib.optionals (hostPort != null) [
      "${hostPort}:${port}" # host:container
    ];
    environment = pkgs.lib.filterAttrs (_: v: v != null) {
      HASURA_CONFIGURATION_DIRECTORY = (pkgs.lib.sources.cleanSource configuration-dir).outPath;
      HASURA_CONNECTOR_PORT = port;
      MONGODB_DATABASE_URI = database-uri;
      OTEL_SERVICE_NAME = "mongodb-connector";
      OTEL_EXPORTER_OTLP_ENDPOINT = otlp-endpoint;
      RUST_LOG = "configuration=debug,mongodb_agent_common=debug,mongodb_connector=debug,mongodb_support=debug,ndc_query_plan=debug";
    };
    volumes = extra-volumes;
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
