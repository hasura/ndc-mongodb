{ pkgs
, hostPort ? "16686" # port for web UI
}:

{
  service = {
    image = "jaegertracing/all-in-one:1.37";
    restart = "always";
    environment = {
      COLLECTOR_OTLP_ENABLED = "true";
      COLLECTOR_ZIPKIN_HOST_PORT = "9411";
    };
    healthcheck = {
      test = [ "CMD" "wget" "--no-verbose" "--tries=1" "--spider" "http://localhost:14269/" ];
      interval = "1s";
      timeout = "3s";
      retries = 25;
    };
    ports = pkgs.lib.optionals (hostPort != null) [
      "${hostPort}:16686"

      # This comment block retained for future reference
      # "5775:5775/udp"
      # "6831:6831/udp"
      # "6832:6832/udp"
      # "5778:5778"
      # "16686:16686" # web UI
      # "14250:14250"
      # "14268:14268"
      # "14269:14269"
      # "4317:4317" # OTLP gRPC
      # "4318:4318" # OTLP HTTP
      # "9411:9411"
    ];
  };
}
