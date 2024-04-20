# Most of these tests assume that you are running in a nix develop shell. You
# can do that by running `$ nix develop`, or by setting up nix-direnv.

default:
  @just --list

test: test-unit test-integration

test-unit:
  cargo test --lib --bins

# This leaves docker containers running - stop them by running `$ arion down`
test-integration:
  arion up -d
  ENGINE_GRAPHQL_URL="http://localhost:7100/graphql" cargo test -p integration-tests

test-ndc: (_arion "arion-compose/project-ndc-test.nix" "test")

test-e2e: (_arion "arion-compose/project-e2e-testing.nix" "test")

# Runs a specified service in a specified project config using arion (a nix
# frontend for docker-compose). Propagates the exit status from that service.
_arion project service:
  arion --file {{project}} run --rm {{service}}; status=$?; arion --file {{project}} down; exit $status
