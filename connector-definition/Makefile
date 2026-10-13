.DEFAULT_GOAL := build
SHELL = /usr/bin/env bash

.PHONY: build
build: dist/connector-definition.tgz

.PHONY: clean
clean:
	rm -rf dist

dist dist/.hasura-connector:
	mkdir dist
	mkdir dist/.hasura-connector

TEMPLATE_VARS := RELEASE_VERSION LINUX_AMD64_SHA256 LINUX_ARM64_SHA256 MACOS_AMD64_SHA256 MACOS_ARM64_SHA256 WINDOWS_AMD64_SHA256

# Validate in the recipe instead of with target-specific `VAR ?= $(error ...)`.
# On GNU make < 4.4 (e.g. 4.3 on GitHub's ubuntu-24.04 runners) those
# assignments remove VAR from the recipe environment, so envsubst silently
# substituted empty strings.
dist/.hasura-connector/connector-metadata.yaml: connector-metadata.yaml dist/.hasura-connector
	$(foreach var,$(TEMPLATE_VARS),$(if $($(var)),,$(error The $(var) variable must be defined)))
	envsubst '$(foreach var,$(TEMPLATE_VARS),$${$(var)})' < connector-metadata.yaml > dist/.hasura-connector/connector-metadata.yaml

dist/connector-definition.tgz: dist/.hasura-connector/connector-metadata.yaml
	shopt -s dotglob && cd dist && tar -czvf connector-definition.tgz *
