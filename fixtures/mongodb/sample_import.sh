#!/bin/bash
#
# Run by the mongo docker image which automatically runs *.sh and *.js scripts
# mounted under /docker-entrypoint-initdb.d/

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

# Sample Claims Data
echo "📡 Importing claims sample data..."
mongoimport --db sample_claims --collection companies --type csv --headerline --file "$FIXTURES"/sample_claims/companies.csv
mongoimport --db sample_claims --collection carriers --type csv --headerline --file "$FIXTURES"/sample_claims/carriers.csv
mongoimport --db sample_claims --collection account_groups --type csv --headerline --file "$FIXTURES"/sample_claims/account_groups.csv
mongoimport --db sample_claims --collection claims --type csv --headerline --file "$FIXTURES"/sample_claims/claims.csv
mongosh sample_claims "$FIXTURES"/sample_claims/view_flat.js
mongosh sample_claims "$FIXTURES"/sample_claims/view_nested.js
echo "✅ Sample claims data imported..."

# mongo_flix
echo "📡 Importing mflix sample data..."
mongoimport --db sample_mflix --collection comments --file "$FIXTURES"/sample_mflix/comments.json
mongoimport --db sample_mflix --collection movies --file "$FIXTURES"/sample_mflix/movies.json
mongoimport --db sample_mflix --collection sessions --file "$FIXTURES"/sample_mflix/sessions.json
mongoimport --db sample_mflix --collection theaters --file "$FIXTURES"/sample_mflix/theaters.json
mongoimport --db sample_mflix --collection users --file "$FIXTURES"/sample_mflix/users.json
echo "✅ Mflix sample data imported..."

# chinook
"$FIXTURES"/chinook/chinook-import.sh
