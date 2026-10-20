# source: https://github.com/bats-core/bats-assert/tree/v2.2.4 and https://github.com/bats-core/bats-support/tree/v0.3.0 (`load.bash`, `src` and LICENSE of each, unchanged, as the fixture directories `bats-assert` and `bats-support`), run by the bats-core fixture
# desc: the assertion libraries most bats suites load, under bats-core: output asserted whole, in part and by regular expression, lines by index and anywhere, refuted, statuses, equality and regular expressions; and failures reported in bats-support's two-column and multi-line formats; under cash's unixpaths, where bats' `load` takes the libraries' paths, which start with `/`, for absolute
# timeout: 90
# unixpaths: $PWD is /c/..., as in Git Bash, so this runs without the C:/ workaround of
# the case it twins; Bash has no such option, and says so on standard error.
shopt -s unixpaths 2>/dev/null
printf '%s\n' \
  'setup() { load "bats-support/load"; load "bats-assert/load"; }' \
  '@test "output whole, in part, by regexp" { run echo "hello world"; assert_output "hello world"; assert_output --partial "lo wo"; assert_output --regexp "^h.*d$"; }' \
  '@test "lines" { run printf "a\nb\nc\n"; assert_line --index 1 b; assert_line c; refute_line z; assert_line --partial --index 2 c; }' \
  '@test "refutes" { run echo x; refute_output y; refute_output --partial yz; refute_output --regexp "^[0-9]+$"; }' \
  '@test "statuses" { run false; assert_failure; assert_failure 1; run true; assert_success; }' \
  '@test "equal and regex" { assert_equal 1 1; assert_not_equal 1 2; assert_regex foo123 "^foo[0-9]+$"; refute_regex abc "[0-9]"; assert [ 1 -lt 2 ]; refute [ 2 -lt 1 ]; }' \
  '@test "fails: output" { run echo actual; assert_output expected; }' \
  '@test "fails: multi-line output" { run printf "one\ntwo\n"; assert_output "one"; }' \
  '@test "fails: equal" { assert_equal a b; }' \
  '@test "fails: line" { run printf "one\ntwo\n"; assert_line three; }' \
  '@test "fails: success" { run sh -c "echo oops; exit 3"; assert_success; }' > t.bats
bash bats-core/bin/bats --formatter tap t.bats; echo "status: $?"
bash bats-core/bin/bats --formatter tap --filter 'lines|statuses' t.bats; echo "filtered: $?"
