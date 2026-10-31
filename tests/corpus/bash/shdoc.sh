# source: https://github.com/reconquest/shdoc/tree/52917b2 (`shdoc`, `tests/testcases` and LICENSE, unchanged, as the fixture directory `shdoc`)
# desc: shdoc, the documentation generator for shell scripts, an awk program: its twenty-six test cases, each a documented script and the Markdown expected of it, run through awk by a stand-in for its test runner (the five `tests:` functions the cases use, defined here), with the diff of any case that differs; its help and version
# timeout: 120
# shdoc's first line asks for gawk; the program is run through awk, which is gawk in
# Git Bash and cash's own in cash, with `--` so that its own options reach it.
mkdir -p run && cp shdoc/shdoc run/shdoc.awk
printf '%s\n' '#!/bin/sh' 'exec awk -f "${0%/*}/shdoc.awk" -- "$@"' > run/shdoc && chmod +x run/shdoc
cd run || exit 1
tests:put() { cat > "$1"; }
tests:ensure() { "$@" > stdout 2> stderr; local status=$?; [ "$status" -eq 0 ] || echo "  $* exited $status"; }
# Blank lines at the end of a file are not compared, as its runner has it: gawk's
# Markdown ends with one more than the cases expect.
trimmed() { awk 'NF { for (; blank; blank--) print ""; print; next } { blank++ }' "$1"; }
tests:assert-no-diff() { if diff <(trimmed "$1") <(trimmed "$2") > diff.out; then echo "  same"; else echo "  differs:"; cat diff.out; fi; }
tests:assert-stdout() { if grep -qF -- "$1" stdout; then echo "  stdout has it"; else echo "  stdout lacks: $1"; fi; }
tests:assert-stdout-re() { if grep -qE -- "$1" stdout; then echo "  stdout matches"; else echo "  stdout lacks: $1"; fi; }
tests:assert-stderr-empty() { if [ -s stderr ]; then echo "  stderr:"; cat stderr; else echo "  stderr empty"; fi; }
# tests/setup.sh's assertion: the input through shdoc, against the expected Markdown.
assert() { tests:ensure ./shdoc < input; tests:assert-no-diff expected stdout; }
for case in ../shdoc/tests/testcases/*.test.sh; do
  echo "== ${case##*/}"
  ( . "$case" )
done
echo "== version"; ./shdoc --version
echo "== an undocumented script"; printf '%s\n' 'f() { :; }' 'g() { :; }' | ./shdoc; echo "status: $?"
