#!/bin/bash

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

# In v6 and later the bundled MongoDB client shell is called "mongosh". In
# earlier versions it's called "mongo".
MONGO_SH=mongosh
if ! command -v mongosh &> /dev/null; then
  MONGO_SH=mongo
fi

echo "📡 Importing claims sample data..."
mongoimport --db sample_claims --collection companies --type csv --headerline --file "$FIXTURES"/companies.csv
mongoimport --db sample_claims --collection carriers --type csv --headerline --file "$FIXTURES"/carriers.csv
mongoimport --db sample_claims --collection account_groups --type csv --headerline --file "$FIXTURES"/account_groups.csv
mongoimport --db sample_claims --collection claims --type csv --headerline --file "$FIXTURES"/claims.csv
$MONGO_SH sample_claims "$FIXTURES"/view_flat.js
$MONGO_SH sample_claims "$FIXTURES"/view_nested.js
echo "✅ Sample claims data imported..."
