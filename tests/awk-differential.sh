#!/usr/bin/env bash
# Differential test comparing Cash native awk against GNU awk (gawk/nawk).
# Usage: ./tests/awk-differential.sh [GNU_AWK_BIN] [CASH_BIN]

set -euo pipefail

GNU_AWK="${1:-awk}"
CASH="${2:-target/debug/cash}"

tmpdir=$(mktemp -d /tmp/cash-awk-diff-XXXXXX)
trap 'rm -rf "$tmpdir"' EXIT

run_case() {
    local name="$1"
    local stdin_data="$2"
    shift 2

    # Run against reference GNU awk
    set +e
    printf "%s" "$stdin_data" | "$GNU_AWK" "$@" > "$tmpdir/ref.out" 2> "$tmpdir/ref.err"
    local ref_code=$?

    # Run through Cash shell using its bundled awk command
    printf "%s" "$stdin_data" | "$CASH" -c 'awk "$@"' awk "$@" > "$tmpdir/cash.out" 2> "$tmpdir/cash.err"
    local cash_code=$?
    set -e

    # Normalize CRLF on cash output
    sed -i 's/\r$//' "$tmpdir/cash.out"
    sed -i 's/\r$//' "$tmpdir/cash.err"

    if ! diff -u "$tmpdir/ref.out" "$tmpdir/cash.out" > "$tmpdir/diff"; then
        echo "FAIL $name: stdout mismatch"
        cat "$tmpdir/diff"
        exit 1
    fi

    if [ "$ref_code" -ne "$cash_code" ]; then
        echo "FAIL $name: exit code mismatch (ref=$ref_code, cash=$cash_code)"
        exit 1
    fi

    echo "PASS $name"
}

# 1. Field splitting and basic printing
run_case "field-split-default" "foo bar baz\n" '{ print $1, $3 }'
run_case "field-split-comma-attached" "apple,banana,cherry\n" -F, '{ print $2 }'
run_case "field-split-colon-space" "root:x:0:0:root:/root:/bin/bash\n" -F ':' '{ print $1, $6 }'

# 2. Variable assignments via -v
run_case "var-assign-space" "" -v "x=42" 'BEGIN { print x * 2 }'
run_case "var-assign-attached" "" -vx=99 'BEGIN { print x + 1 }'

# 3. Arithmetic and built-in math
run_case "math-builtins" "" 'BEGIN { print sqrt(16), int(7.8), length("hello world") }'

# 4. String manipulation
run_case "string-builtins" "banana split\n" '{
    print substr($1, 1, 3), index($1, "na"), match($1, /nan/)
}'
run_case "string-sub-gsub" "a-b-c-d\n" '{
    s = $0;
    sub(/-/, ":", s);
    print s;
    gsub(/-/, "_", $0);
    print $0;
}'

# 5. Arrays, iteration and delete
run_case "arrays-and-delete" "" 'BEGIN {
    a["x"] = 10;
    a["y"] = 20;
    a["z"] = 30;
    delete a["y"];
    for (k in a) {
        # sort order might vary, but both x and z exist and y is deleted
        if (k == "x" || k == "z") print k, a[k];
    }
}'

# 6. NF, NR, FNR record variables
run_case "record-variables" "line one\nline two three\n" '{
    print NR, FNR, NF, $NF
}'

# 7. BEGIN and END blocks
run_case "begin-end" "10\n20\n30\n" 'BEGIN { sum = 0 } { sum += $1 } END { print "TOTAL", sum }'

echo "All awk differential tests passed."
