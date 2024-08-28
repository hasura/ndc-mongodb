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

echo "📡 Importing mflix sample data..."
mongoimport --db sample_mflix --collection comments --file "$FIXTURES"/comments.json
mongoimport --db sample_mflix --collection movies --file "$FIXTURES"/movies.json
mongoimport --db sample_mflix --collection sessions --file "$FIXTURES"/sessions.json
mongoimport --db sample_mflix --collection theaters --file "$FIXTURES"/theaters.json
mongoimport --db sample_mflix --collection users --file "$FIXTURES"/users.json
$MONGO_SH sample_mflix "$FIXTURES/indexes.js"
echo "✅ Mflix sample data imported..."
