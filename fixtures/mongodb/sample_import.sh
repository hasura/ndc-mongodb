#!/bin/bash
#
# Run by the mongo docker image which automatically runs *.sh and *.js scripts
# mounted under /docker-entrypoint-initdb.d/

set -euo pipefail

# Get the directory of this script file
FIXTURES=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )

"$FIXTURES"/sample_claims/import.sh
"$FIXTURES"/sample_mflix/import.sh
"$FIXTURES"/chinook/chinook-import.sh
"$FIXTURES"/test_cases/import.sh
