{ pkgs
, port ? "7100"
, hostPort ? null
, connectors ? [{ name = "sample_mflix"; url = "http://connector:7130"; subgraph = ../fixtures/ddn/subgraphs/sample_mflix; }]
, auth-webhook ? { url = "http://auth-hook:3050/validate-request"; }
, otlp-endpoint ? "http://jaeger:4317"
, service ? { } # additional options to customize this service configuration
}:

let
  # Compile JSON metadata from HML fixture
  metadata = pkgs.stdenv.mkDerivation {
    name = "hasura-metadata.json";
    src = (builtins.head connectors).subgraph;
    nativeBuildInputs = with pkgs; [ findutils jq yq-go ];

    # The yq command converts the input sequence of yaml docs to a sequence of
    # newline-separated json docs.
    #
    # The jq command combines those json docs into an array (due to the -s
    # switch), and modifies the json to update the data connector url.
    buildPhase = ''
      combined=$(mktemp -t subgraph-XXXXXX.hml)
      for obj in $(find . -name '*hml'); do
        echo "---" >> "$combined"
        cat "$obj" >> "$combined"
      done
      cat "$combined" \
        | yq -o=json \
        ${connector-url-substituters} \
        | jq -s 'map(select(type != "null"))' \
        > metadata.json
    '';

    installPhase = ''
      cp metadata.json "$out"
    '';
  };

  # Pipe commands to replace data connector urls in fixture configuration with
  # urls of dockerized connector instances
  connector-url-substituters = builtins.toString (builtins.map
    ({ name, url, ... }:
      '' | jq 'if .kind == "DataConnectorLink" and .definition.name == "${name}" then .definition.url = { singleUrl: { value: "${url}" } } else . end' ''
    )
    connectors);

  auth-config = pkgs.writeText "auth_config.json" (builtins.toJSON {
    version = "v1";
    definition = {
      mode.webhook = {
        url = auth-webhook.url;
        method = "Post";
      };
    };
  });

  withOverrides = overrides: config: pkgs.lib.attrsets.recursiveUpdate config overrides;
in
{
  image.enableRecommendedContents = true;
  image.contents = with pkgs.pkgsCross.linux; [
    cacert
    curl
    graphql-engine # added to pkgs via an overlay in flake.nix.
  ];
  service = withOverrides service {
    useHostStore = true;
    command = [
      "engine"
      "--port=${port}"
      "--metadata-path=${metadata}"
      "--authn-config-path=${auth-config}"
    ] ++ (pkgs.lib.optionals (otlp-endpoint != null) [
      "--otlp-endpoint=${otlp-endpoint}"
    ]);
    ports = pkgs.lib.optionals (hostPort != null) [
      "${hostPort}:${port}"
    ];
    environment = {
      RUST_LOG = "engine=debug,hasura-authn-core=debug";
    };
    healthcheck = {
      test = [ "CMD" "curl" "-f" "http://localhost:${port}/" ];
      start_period = "5s";
      interval = "5s";
      timeout = "1s";
      retries = 3;
    };
  };
}
