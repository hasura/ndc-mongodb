{ name ? "ghcr.io/hasura/mongodb-cli-plugin"
, mongodb-cli-plugin
, dockerTools
}:

dockerTools.buildLayeredImage {
  inherit name;
  created = "now";
  config = {
    Entrypoint = [ "${mongodb-cli-plugin}/bin/hasura-mongodb" ];
  };
}
