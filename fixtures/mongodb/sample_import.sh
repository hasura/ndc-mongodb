#!/bin/bash
#
# Run by the mongo docker image which automatically runs *.sh and *.js scripts
# mounted under /docker-entrypoint-initdb.d/

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

# Sample Claims Data
"$FIXTURES"/sample_claims/import.sh

# mongo_flix
"$FIXTURES"/sample_mflix/import.sh

# chinook
"$FIXTURES"/chinook/chinook-import.sh
