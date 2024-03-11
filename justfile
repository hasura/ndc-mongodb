# Most of these tests assume that you are running in a nix develop shell. You
# can do that by running `$ nix develop`, or by setting up nix-direnv.

default:
  @just --list

test: test-unit test-ndc test-e2e

test-unit:
  cargo test

test-ndc: (_arion "arion-compose/project-ndc-test.nix" "test")

test-e2e: (_arion "arion-compose/project-e2e-testing.nix" "test")

# Runs a specified service in a specified project config using arion (a nix
# frontend for docker-compose). Propagates the exit status from that service.
_arion project service:
  arion --file {{project}} run --rm {{service}}; status=$?; arion --file {{project}} down; exit $status
