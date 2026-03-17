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

dist/.hasura-connector/connector-metadata.yaml: connector-metadata.yaml dist/.hasura-connector
	@test -n "$${RELEASE_VERSION}" || (echo "ERROR: RELEASE_VERSION is not set" && exit 1)
	@test -n "$${LINUX_AMD64_SHA256}" || (echo "ERROR: LINUX_AMD64_SHA256 is not set" && exit 1)
	@test -n "$${LINUX_ARM64_SHA256}" || (echo "ERROR: LINUX_ARM64_SHA256 is not set" && exit 1)
	@test -n "$${MACOS_AMD64_SHA256}" || (echo "ERROR: MACOS_AMD64_SHA256 is not set" && exit 1)
	@test -n "$${MACOS_ARM64_SHA256}" || (echo "ERROR: MACOS_ARM64_SHA256 is not set" && exit 1)
	@test -n "$${WINDOWS_AMD64_SHA256}" || (echo "ERROR: WINDOWS_AMD64_SHA256 is not set" && exit 1)
	envsubst '$${RELEASE_VERSION}$${LINUX_AMD64_SHA256}$${MACOS_AMD64_SHA256}$${WINDOWS_AMD64_SHA256}$${LINUX_ARM64_SHA256}$${MACOS_ARM64_SHA256}' < connector-metadata.yaml > dist/.hasura-connector/connector-metadata.yaml

dist/connector-definition.tgz: dist/.hasura-connector/connector-metadata.yaml
	cd dist && tar -czvf connector-definition.tgz .hasura-connector
