# Run commands in a nix develop shell by default which provides commands like
# `arion`.
set shell := ["nix", "--experimental-features", "nix-command flakes", "develop", "--command", "bash", "-c"]

# Display available recipes
default:
  @just --list

# Run a local development environment using docker. This makes the GraphQL
# Engine available on https://localhost:7100/ with two connected MongoDB
# connector instances.
up:
  arion up -d

# Stop the local development environment docker containers.
down:
  arion down

# Stop the local development environment docker containers, and remove volumes.
down-volumes:
  arion down --volumes

# Output logs from local development environment services.
logs:
  arion logs

test: test-unit test-integration

test-unit:
  cargo test

test-integration: (_arion "arion-compose/integration-tests.nix" "test")

test-ndc: (_arion "arion-compose/ndc-test.nix" "test")

test-e2e: (_arion "arion-compose/e2e-testing.nix" "test")

# Run `just test-integration` on several MongoDB versions
test-mongodb-versions:
  MONGODB_IMAGE=mongo:6 just test-integration
  MONGODB_IMAGE=mongo:7 just test-integration
  MONGODB_IMAGE=mongo:8 just test-integration

# Runs a specified service in a specified project config using arion (a nix
# frontend for docker-compose). Propagates the exit status from that service.
_arion project service:
  arion --file {{project}} run --rm {{service}}; status=$?; arion --file {{project}} down; exit $status
