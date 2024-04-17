#!/bin/bash

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
DATABASE_NAME=chinook

echo "📡 Importing Chinook into database $DATABASE_NAME..."

importCollection() {
  local collection="$1"
  local schema_file="$FIXTURES/$collection.schema.json"
  local data_file="$FIXTURES/$collection.data.json"
  echo "🔐 Applying validation for ${collection}..."
    mongosh --eval "
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

echo "✅ Sample Chinook data imported..."
