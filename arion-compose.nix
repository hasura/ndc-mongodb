# Arion is a Nix frontend to docker-compose. That is helpful for development
# because it automatically builds and runs the agent using flake configuration
# so that we don't have to manually build and install a new docker image between
# code changes.
#
# This module effectively compiles to a docker-compose.yaml file. But instead of
# running with docker-compose, use commands like:
#
#     $ arion up -d        # to start everything
#     $ arion up -d agent  # to recompile and restart the agent service
#
# The `arion` command delegates to docker-compose so it uses the same
# sub-commands and flags. Arion is included in the flake.nix devShell, so if you
# have Nix and direnv-nix set up then it should be available automatically.
#
# For options that can be used in this file see https://docs.hercules-ci.com/arion/options
# In general docker-compose.yaml options can be used here by converting from
# yaml to nix syntax.
#
# For a more general description of Arion see https://docs.hercules-ci.com/arion/
#
# This repo provides multiple "projects" - the equivalent of multiple
# `docker-compose.yaml` configurations for different purposes. This one is run
# by default, and delegates to `arion-compose/project-v2.nix`. Run a different
# project like this:
#
#     arion -f arion-compose/project-v3.nix up -d
#

import ./arion-compose/project-connector.nix
