# Most of these tests assume that you are running in a nix develop shell. You
# can do that by running `$ nix develop`, or by setting up nix-direnv.

default:
  @just --list

test: test-unit test-integration

test-unit:
  cargo test

test-integration: (_arion "arion-compose/integration-tests.nix" "test")

test-ndc: (_arion "arion-compose/ndc-test.nix" "test")

test-e2e: (_arion "arion-compose/e2e-testing.nix" "test")

# Run `just test-integration` on several MongoDB versions
test-mongodb-versions:
  MONGODB_IMAGE=mongo:4 just test-integration
  # MONGODB_IMAGE=mongo:5 just test-integration # there's a problem with the native query example in v5
  MONGODB_IMAGE=mongo:6 just test-integration
  MONGODB_IMAGE=mongo:7 just test-integration

# Runs a specified service in a specified project config using arion (a nix
# frontend for docker-compose). Propagates the exit status from that service.
_arion project service:
  arion --file {{project}} run --rm {{service}}; status=$?; arion --file {{project}} down; exit $status
