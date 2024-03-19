#!/usr/bin/env bash

set -evo pipefail
ROOT="$(pwd)"

export LINUX_AMD64_SHA256=$(cat ${ROOT}/release/artifacts/sha256sum   | grep mongodb-cli-plugin-x86_64-unknown-linux-musl  | cut -f1 -d' ')
export MACOS_AMD64_SHA256=$(cat ${ROOT}/release/artifacts/sha256sum   | grep mongodb-cli-plugin-x86_64-apple-darwin        | cut -f1 -d' ')
export WINDOWS_AMD64_SHA256=$(cat ${ROOT}/release/artifacts/sha256sum | grep mongodb-cli-plugin-x86_64-pc-windows-msvc.exe | cut -f1 -d' ')
export LINUX_ARM64_SHA256=$(cat ${ROOT}/release/artifacts/sha256sum   | grep mongodb-cli-plugin-aarch64-unknown-linux-musl | cut -f1 -d' ')
export MACOS_ARM64_SHA256=$(cat ${ROOT}/release/artifacts/sha256sum   | grep mongodb-cli-plugin-aarch64-apple-darwin       | cut -f1 -d' ')

(echo "cat <<EOF >${ROOT}/release/manifest.yaml";
cat ${ROOT}/scripts/manifest.yaml; echo; echo EOF;
)>${ROOT}/release/manifest-tmp.yaml
. ${ROOT}/release/manifest-tmp.yaml