#!/usr/bin/env bash
# Differential test comparing Cash native sed against GNU sed.
# Usage: ./tests/sed-differential.sh [GNU_SED_BIN] [CASH_BIN]

set -euo pipefail

GNU_SED="${1:-sed}"
CASH="${2:-target/debug/cash.exe}"

tmpdir=$(mktemp -d /tmp/cash-sed-diff-XXXXXX)
trap 'rm -rf "$tmpdir"' EXIT

run_case() {
    local name="$1"
    local stdin_data="$2"
    shift 2

    # Run against reference GNU sed
    set +e
    printf "%b" "$stdin_data" | "$GNU_SED" "$@" > "$tmpdir/ref.out" 2> "$tmpdir/ref.err"
    local ref_code=$?

    # Run through Cash shell using its bundled sed command
    printf "%b" "$stdin_data" | "$CASH" -c 'sed "$@"' sed "$@" > "$tmpdir/cash.out" 2> "$tmpdir/cash.err"
    local cash_code=$?
    set -e

    # Normalize CRLF on cash output
    sed -i 's/\r$//' "$tmpdir/cash.out" 2>/dev/null || true
    sed -i 's/\r$//' "$tmpdir/cash.err" 2>/dev/null || true

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

echo "=== Running Classic & Esoteric Sed Differential Suite ==="

# 1. Classic tac (reverse lines of a file using hold space)
run_case "classic-tac-reverse-lines" "first\nsecond\nthird\nfourth\n" '1!G;h;$!d'

# 2. Classic rev (reverse characters on each line)
run_case "classic-rev-characters" "hello world\n12345\n" '/\n/!G;s/\(.\)\(.*\n\)/&\2\1/;//D;s/.//'

# 3. Classic uniq (delete duplicate consecutive lines)
run_case "classic-uniq-duplicate-lines" "a\na\nb\nc\nc\nc\nd\n" '$!N; /^\(.*\)\n\1$/!P; D'

# 4. Line numbering (emulate wc -l / count lines)
run_case "classic-count-lines" "alpha\nbeta\ngamma\ndelta\n" -n '$='

# 5. Join pairs of lines side-by-side
run_case "classic-join-pairs" "one\ntwo\nthree\nfour\n" '$!N;s/\n/ /'

# 6. Emulate head -n 3
run_case "classic-head-3" "1\n2\n3\n4\n5\n6\n" '3q'

# 7. Emulate tail -n 2 (sliding 2-line window in pattern space)
run_case "classic-tail-2" "1\n2\n3\n4\n5\n6\n" '$!N;$!D'

# 8. ROT13 cipher transliteration
run_case "classic-rot13" "Hello, World! 123\n" 'y/abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ/nopqrstuvwxyzabcdefghijklmNOPQRSTUVWXYZABCDEFGHIJKLM/'

# 9. Format numbers with commas (loop with label :a and branch ta)
run_case "classic-comma-numbers" "1234567\n1000\n42\n9876543210\n" ':a;s/\B[0-9]\{3\}\>/,&/;ta'

# 10. Strip HTML tags
run_case "classic-strip-html" "<p>This is <b>bold</b> and <a href=\"#\">a link</a>.</p>\n" 's/<[^>]*>//g'

# 11. Numbered substitution occurrence (replace only 3rd occurrence)
run_case "subst-3rd-occurrence" "foo foo foo foo foo\n" 's/foo/BAR/3'

# 12. Numbered substitution occurrence with global (replace from 2nd occurrence onward)
run_case "subst-2nd-and-after" "x x x x x\n" 's/x/Y/2g'

# 13. Swap adjacent lines (h;n;G;p cycle)
run_case "swap-adjacent-lines" "line1\nline2\nline3\nline4\n" -n 'h;n;G;p'

# 14. POSIX leftmost-longest alternation in ERE
run_case "ere-leftmost-longest" "aa\n" -E 's/a|aa/X/'

# 15. Backreference palindrome detection
run_case "ere-backref-palindrome" "racecar\nradar\nhello\nlevel\n" -E -n '/^(.)(.)(.).\3\2\1$/p'

# 16. Sed increment arithmetic (increment single-digit and carry to 10)
run_case "classic-increment-math" "7\n8\n9\n" '/[0-8]$/{s/0$/1/;s/1$/2/;s/2$/3/;s/3$/4/;s/4$/5/;s/5$/6/;s/6$/7/;s/7$/8/;s/8$/9/;b};s/9$/0/;s/\([^0-9]\)\?$/\11/'

# 17. Custom delimiters (# and |)
run_case "custom-delimiter-hash" "http://example.com/test\n" 's#http://#https://#g'
run_case "custom-delimiter-pipe" "/usr/local/bin:/usr/bin\n" 's|/usr/local/bin|/opt/bin|g'

# 18. Inverted address range (2,3!d)
run_case "inverted-address-range" "one\ntwo\nthree\nfour\nfive\n" '2,3!d'

# 19. Delete blank lines
run_case "delete-blank-lines" "hello\n\nworld\n\n\nfoo\n" '/^$/d'

# 20. Double space output
run_case "double-space-lines" "line1\nline2\nline3\n" 'G'

# 21. Squeeze consecutive blank lines (cat -s emulation)
run_case "squeeze-blank-lines" "line1\n\n\n\nline2\n\nline3\n" '/^$/{N;/^\n$/D;}'

# 22. Join all lines into single space-delimited line
run_case "join-all-lines" "one\ntwo\nthree\nfour\n" -e ':a' -e '$!N;s/\n/ /;ta'

# 23. Grep with context -A 2
run_case "grep-context-a2" "a\nb\nTARGET\nc\nd\ne\n" -n '/TARGET/{p;n;p;n;p;}'

# 24. Pattern range inclusive
run_case "range-inclusive" "ignore\nSTART\ninside1\ninside2\nEND\nignore\n" -n '/START/,/END/p'

# 25. Pattern range exclusive (//!p)
run_case "range-exclusive" "ignore\nSTART\ninside1\ninside2\nEND\nignore\n" -n '/START/,/END/{//!p;}'

# 26. Delete trailing spaces and tabs
run_case "delete-trailing-whitespace" "hello   \nworld\t\t\nclean\n" 's/[ \t]*$//'

# 27. Exchange pattern space with hold space (x)
run_case "hold-exchange-x" "first\nsecond\n" 'x'

# 28. Unary conversion / loop
run_case "unary-roman-loop" "IIIIIIIIII\n" -e ':a;s/IIIII/V/g;ta'

# 29. Print only first and last line
run_case "first-and-last-line" "line1\nline2\nline3\nline4\n" -n '1p;$p'

# 30. Multiple sequential -e expressions
run_case "multi-e-cascade" "apple\n" -e 's/a/b/' -e 's/p/x/g' -e 's/e/z/'

echo "=== All 30 Sed Differential Cases Passed! ==="
