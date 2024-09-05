#!/bin/bash
#
# Populates the test_cases mongodb database. When writing integration tests we
# come up against cases where we want some specific data to test against that
# doesn't exist in the sample_mflix or chinook databases. Such data can go into
# the test_cases database as needed.

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

echo "📡 Importing test case data..."
mongoimport --db test_cases --collection weird_field_names --file "$FIXTURES"/weird_field_names.json
echo "✅ test case data imported..."

