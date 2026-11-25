# This is a function that returns a derivation for a docker image.
{ mongodb-connector
, cacert
, runCommand
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

  # Vendored Amazon RDS / DocumentDB global CA bundle (not included in Mozilla's cacert).
  # Source: https://truststore.pki.rds.amazonaws.com/global/global-bundle.pem
  awsRdsCaBundle = ../certs/aws-rds-global-bundle.pem;

  # Combine Nix's Mozilla cacert with Amazon RDS / DocumentDB CA roots so both
  # public CAs (e.g. MongoDB Atlas) and AWS DocumentDB / RDS are trusted out of the box.
  combinedCacert = runCommand "cacert-with-aws-rds" { } ''
    mkdir -p $out/etc/ssl/certs
    cat ${cacert}/etc/ssl/certs/ca-bundle.crt ${awsRdsCaBundle} > $out/etc/ssl/certs/ca-bundle.crt
    ln -s ca-bundle.crt $out/etc/ssl/certs/ca-certificates.crt
  '';

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
        "SSL_CERT_FILE=/etc/ssl/certs/ca-bundle.crt"
      ];
    } // extraConfig;
    contents = [ combinedCacert ]; # include Mozilla + Amazon RDS/DocumentDB TLS root CAs
  };
in
dockerTools.buildLayeredImage args
