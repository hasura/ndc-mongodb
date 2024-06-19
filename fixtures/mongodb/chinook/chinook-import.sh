#!/bin/bash

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
DATABASE_NAME=chinook

# In v6 and later the bundled MongoDB client shell is called "mongosh". In
# earlier versions it's called "mongo".
MONGO_SH=mongosh
if ! command -v mongosh &> /dev/null; then
  MONGO_SH=mongo
fi

echo "📡 Importing Chinook into database $DATABASE_NAME..."

importCollection() {
  local collection="$1"
  local schema_file="$FIXTURES/$collection.schema.json"
  local data_file="$FIXTURES/$collection.data.json"
  echo "🔐 Applying validation for ${collection}..."
    $MONGO_SH --eval "
        var schema = $(cat "${schema_file}");
        db.createCollection('${collection}', { validator: schema });
    " "$DATABASE_NAME"

  echo "⬇️ Importing data for ${collection}..."
  mongoimport --db "$DATABASE_NAME" --collection "$collection" --type json --jsonArray --file "$data_file"
}

importCollection "Album"
importCollection "Artist"
importCollection "Customer"
importCollection "Employee"
importCollection "Genre"
importCollection "Invoice"
importCollection "InvoiceLine"
importCollection "MediaType"
importCollection "Playlist"
importCollection "PlaylistTrack"
importCollection "Track"

$MONGO_SH "$DATABASE_NAME" "$FIXTURES/indexes.js"

echo "✅ Sample Chinook data imported..."
