#!/bin/bash

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

echo "📡 Importing Chinook..."

loadSchema() {
    local collection="$1"
    local schema_file="$2"
    echo "🔐 Applying validation for ${collection}..."
    mongosh --eval "
        var schema = $(cat "${schema_file}");
        db.createCollection('${collection}', { validator: schema });
    " chinook
}

loadSchema "Album" "$FIXTURES/Album.json"
loadSchema "Artist" "$FIXTURES/Artist.json"
loadSchema "Customer" "$FIXTURES/Customer.json"
loadSchema "Employee" "$FIXTURES/Employee.json"
loadSchema "Genre" "$FIXTURES/Genre.json"
loadSchema "Invoice" "$FIXTURES/Invoice.json"
loadSchema "InvoiceLine" "$FIXTURES/InvoiceLine.json"
loadSchema "MediaType" "$FIXTURES/MediaType.json"
loadSchema "Playlist" "$FIXTURES/Playlist.json"
loadSchema "PlaylistTrack" "$FIXTURES/PlaylistTrack.json"
loadSchema "Track" "$FIXTURES/Track.json"

mongoimport --db chinook --collection Album --type csv --headerline --file "$FIXTURES"/Album.csv
mongoimport --db chinook --collection Artist --type csv --headerline --file "$FIXTURES"/Artist.csv
mongoimport --db chinook --collection Customer --type csv --headerline --file "$FIXTURES"/Customer.csv
mongoimport --db chinook --collection Employee --type csv --headerline --file "$FIXTURES"/Employee.csv
mongoimport --db chinook --collection Genre --type csv --headerline --file "$FIXTURES"/Genre.csv
mongoimport --db chinook --collection Invoice --type csv --headerline --file "$FIXTURES"/Invoice.csv
mongoimport --db chinook --collection InvoiceLine --type csv --headerline --file "$FIXTURES"/InvoiceLine.csv
mongoimport --db chinook --collection MediaType --type csv --headerline --file "$FIXTURES"/MediaType.csv
mongoimport --db chinook --collection Playlist --type csv --headerline --file "$FIXTURES"/Playlist.csv
mongoimport --db chinook --collection PlaylistTrack --type csv --headerline --file "$FIXTURES"/PlaylistTrack.csv
mongoimport --db chinook --collection Track --type csv --headerline --file "$FIXTURES"/Track.csv

echo "✅ Sample Chinook data imported..."
