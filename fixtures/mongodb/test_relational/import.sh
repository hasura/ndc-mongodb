#!/bin/bash
#
# Populates the test_relational mongodb database used by the relational query
# integration tests.

set -euo pipefail

FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

echo "📡 Importing relational test data..."
for fixture in "$FIXTURES"/*.json; do
    collection=$(basename "$fixture" .json)
    mongoimport --drop --db test_relational --collection "$collection" --file "$fixture"
done
echo "✅ relational test data imported..."
