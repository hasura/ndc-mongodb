# This is a function that returns a derivation for a docker image.
{ mongodb-connector
, dockerTools
, name ? "ghcr.io/hasura/ndc-mongodb"

  # See config options at https://github.com/moby/docker-image-spec/blob/main/spec.md
, extraConfig ? { }
}:

let
  config-directory = "/etc/connector";
  default-port = "8080";
  default-database-uri = "mongodb://localhost/db";
  default-otlp-endpoint = "http://localhost:4317";

  args = {
    inherit name;
    created = "now";
    config = {
      Entrypoint = [ "${mongodb-connector}/bin/mongodb-connector" ];
      Cmd = [ "serve" ];
      ExposedPorts = {
        "${default-port}/tcp" = { };
      };
      Env = [
        "HASURA_CONFIGURATION_DIRECTORY=${config-directory}"
        "HASURA_CONNECTOR_PORT=${default-port}"
        "MONGODB_DATABASE_URI=${default-database-uri}"
        "OTEL_SERVICE_NAME=mongodb-connector"
        "OTEL_EXPORTER_OTLP_ENDPOINT=${default-otlp-endpoint}"
      ];
    } // extraConfig;
  };
in
dockerTools.buildLayeredImage args
