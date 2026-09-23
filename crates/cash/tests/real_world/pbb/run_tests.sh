#!/usr/bin/env bash
set -e

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$DIR"

# Extract pure-bash-bible functions from README.md
while IFS= read -r line; do
    if [[ "$code" == 1 ]]; then
        if [[ "$line" == '```' ]]; then
            code=0
        else
            printf '%s\n' "$line"
        fi
    elif [[ "$line" == '```sh' ]]; then
        code=1
    fi
done < README.md > readme_code.sh

# Source the extracted functions
. ./readme_code.sh

# Load test assertions and tests from test.sh
. ./test.sh

pass=0
fail=0

# Run string and data tests
test_trim_string
test_trim_all
test_lower
test_upper
test_reverse_case
test_trim_quotes
test_lstrip
test_rstrip
test_urlencode
test_urldecode

rm -f readme_code.sh

echo "ALL TESTS COMPLETED: pass=$pass fail=$fail"
if (( fail > 0 )); then
    exit 1
fi
exit 0
