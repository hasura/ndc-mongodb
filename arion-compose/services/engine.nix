{ pkgs
, port ? "7100"
, hostPort ? null

  # Each key in the `connectors` map should match
  # a `DataConnectorLink.definition.name` value in one of the given `ddn-dirs`
  # to correctly match up configuration to connector instances.
, connectors ? { sample_mflix = "http://connector:7130"; }
, ddn-dirs ? [ ../../fixtures/hasura/sample_mflix/metadata ]
, auth-webhook ? { url = "http://auth-hook:3050/validate-request"; }
, otlp-endpoint ? "http://jaeger:4317"
, service ? { } # additional options to customize this service configuration
}:

let
  # Compile JSON metadata from HML fixtures
  #
  # Converts yaml documents from each ddn-dir into json objects, and combines
  # objects into one big array. Produces a file in the Nix store of the form
  # /nix/store/<hash>-hasura-metadata.json
  metadata = pkgs.runCommand "hasura-metadata.json" { } ''
    ${pkgs.jq}/bin/jq -s 'flatten(1)' \
      ${builtins.concatStringsSep " " (builtins.map compile-ddn ddn-dirs)} \
      > $out
  '';

  # Translate each yaml document from hml files into a json object, and combine
  # all objects into an array
  compile-ddn = ddn-dir: pkgs.stdenv.mkDerivation {
    name = "ddn-${builtins.baseNameOf ddn-dir}.json";
    src = ddn-dir;
    nativeBuildInputs = with pkgs; [ findutils jq yq-go ];

    # The yq command converts the input sequence of yaml docs to a sequence of
    # newline-separated json docs.
    #
    # The jq command combines those json docs into an array (due to the -s
    # switch), and modifies the json to update the data connector url.
    buildPhase = ''
      combined=$(mktemp -t ddn-${builtins.baseNameOf ddn-dir}-XXXXXX.hml)
      for obj in $(find . -name '*hml'); do
        echo "---" >> "$combined"
        cat "$obj" >> "$combined"
      done
      cat "$combined" \
        | yq -o=json \
        ${connector-url-substituters} \
        | jq -s 'map(select(type != "null"))' \
        > ddn.json
    '';

    installPhase = ''
      cp ddn.json "$out"
    '';
  };

  # Pipe commands to replace data connector urls in fixture configuration with
  # urls of dockerized connector instances
  connector-url-substituters = builtins.toString (builtins.attrValues (builtins.mapAttrs
    (name: url:
      '' | jq 'if .kind == "DataConnectorLink" and .definition.name == "${name}" then .definition.url = { singleUrl: { value: "${url}" } } else . end' ''
    )
    connectors));

  auth-config = pkgs.writeText "auth_config.json" (builtins.toJSON {
    version = "v2";
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
      "--expose-internal-errors"
    ] ++ (pkgs.lib.optionals (otlp-endpoint != null) [
      "--otlp-endpoint=${otlp-endpoint}"
    ]);
    ports = pkgs.lib.optionals (hostPort != null) [
      "${hostPort}:${port}"
    ];
    environment = {
      RUST_LOG = "engine=debug,hasura_authn_core=debug,hasura_authn_jwt=debug,hasura_authn_noauth=debug,hasura_authn_webhook=debug,lang_graphql=debug,open_dds=debug,schema=debug,metadata-resolve=debug";
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
