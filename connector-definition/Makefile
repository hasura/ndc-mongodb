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

dist/.hasura-connector/connector-metadata.yaml: RELEASE_VERSION ?= $(error The RELEASE_VERSION variable must be defined)
dist/.hasura-connector/connector-metadata.yaml: LINUX_AMD64_SHA256 ?= $(error The LINUX_AMD64_SHA256 variable must be defined)
dist/.hasura-connector/connector-metadata.yaml: LINUX_ARM64_SHA256 ?= $(error The LINUX_ARM64_SHA256 variable must be defined)
dist/.hasura-connector/connector-metadata.yaml: MACOS_AMD64_SHA256 ?= $(error The MACOS_AMD64_SHA256 variable must be defined)
dist/.hasura-connector/connector-metadata.yaml: MACOS_ARM64_SHA256 ?= $(error The MACOS_ARM64_SHA256 variable must be defined)
dist/.hasura-connector/connector-metadata.yaml: WINDOWS_AMD64_SHA256 ?= $(error The WINDOWS_AMD64_SHA256 variable must be defined)
dist/.hasura-connector/connector-metadata.yaml: connector-metadata.yaml dist/.hasura-connector
	envsubst '$${RELEASE_VERSION}$${LINUX_AMD64_SHA256}$${MACOS_AMD64_SHA256}$${WINDOWS_AMD64_SHA256}$${LINUX_ARM64_SHA256}$${MACOS_ARM64_SHA256}' < connector-metadata.yaml > dist/.hasura-connector/connector-metadata.yaml

dist/connector-definition.tgz: dist/.hasura-connector/connector-metadata.yaml
	shopt -s dotglob && cd dist && tar -czvf connector-definition.tgz *
