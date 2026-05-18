#!/bin/sh
# Loads key/value pairs from every JSON file in /secrets/ as environment
# variables, then exec's the original command. Lets the connector consume
# external secrets (e.g. ESO ExternalSecret) mounted as JSON files without
# requiring the upstream image to know about that layout.
set -e

SECRETS_DIR="${SECRETS_DIR:-/secrets}"

if [ -d "$SECRETS_DIR" ]; then
  for f in "$SECRETS_DIR"/*.json; do
    [ -f "$f" ] || continue
    eval "$(jq -r 'to_entries | .[] | "export \(.key)=\(.value | @sh)"' "$f")"
  done
fi

exec "$@"
