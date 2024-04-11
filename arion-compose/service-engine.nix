{ pkgs
, port ? "7100"
, hostPort ? null
, connector-url ? "http://connector:7130"
, ddn-subgraph-dir ? ../fixtures/ddn/subgraphs/chinook
, auth-webhook ? { url = "http://auth-hook:3050/validate-request"; }
, otlp-endpoint ? "http://jaeger:4317"
, service ? { } # additional options to customize this service configuration
}:

let
  # Compile JSON metadata from HML fixture
  metadata = pkgs.stdenv.mkDerivation {
    name = "hasura-metadata.json";
    src = ddn-subgraph-dir;
    nativeBuildInputs = with pkgs; [ jq yq-go ];

    # The yq command converts the input sequence of yaml docs to a sequence of
    # newline-separated json docs.
    #
    # The jq command combines those json docs into an array (due to the -s
    # switch), and modifies the json to update the data connector url.
    buildPhase = ''
      combined=$(mktemp -t subgraph-XXXXXX.hml)
      for obj in **/*.hml; do
        echo "---" >> "$combined"
        cat "$obj" >> "$combined"
      done
      cat "$combined" \
        | yq -o=json \
        | jq -s 'map(if .kind == "DataConnectorLink" then .definition.url = { singleUrl: { value: "${connector-url}" } } else . end)' \
        > metadata.json
    '';

    installPhase = ''
      cp metadata.json "$out"
    '';
  };

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
