# source: https://github.com/bash-unit/bash_unit/tree/v2.3.3 (`bash_unit` and `tests/test_core.sh`, unchanged, with its LICENSE, as the fixture directory `bash_unit`)
# desc: bash_unit, its own core suite (assert, assert_fails, assert_equals and the rest, fake, the working directory a test runs in) and a suite of the case's: setup and teardown, setup_suite and teardown_suite, faked commands with their arguments and standard input, todo and skip, failures with their messages and stack lines, a pattern, a skip pattern and the TAP format
# timeout: 120
cd bash_unit || exit 1
bash ./bash_unit tests/test_core.sh < /dev/null; echo "core: $?"
printf '%s\n' \
  'setup_suite() { export SUITE=ready; }' \
  'teardown_suite() { echo "suite done" >&2; }' \
  'setup() { scratch=$(mktemp -d); }' \
  'teardown() { rm -rf "$scratch"; }' \
  'test_setup_ran() { assert "[ -d \"$scratch\" ]"; assert_equals ready "$SUITE"; }' \
  'test_equals() { assert_equals "a b" "a b"; assert_not_equals a b; }' \
  'test_matches() { assert_matches "^v[0-9]+$" v42; assert_not_matches "^x" abc; }' \
  'test_status() { assert_status_code 3 "exit 3"; assert_fails false; }' \
  'test_within_delta() { assert_within_delta 10 12 2; }' \
  'test_no_diff() { echo same > "$scratch/a"; echo same > "$scratch/b"; assert_no_diff "$scratch/a" "$scratch/b"; }' \
  'test_fake_with_args() { fake ps echo "faked ps: \$FAKE_PARAMS"; assert_equals "faked ps: -ef" "$(ps -ef)"; }' \
  'test_fake_reads_stdin() { fake wc _wc; _wc() { echo "lines: $(cat | grep -c .)"; }; export -f _wc; assert_equals "lines: 2" "$(printf "a\nb\n" | wc)"; }' \
  'todo_later() { fail "not yet"; }' \
  'test_skipped_by_pattern() { fail "should not run"; }' \
  'test_fails_equals() { assert_equals expected actual "a message"; }' \
  'test_fails_assert() { assert "test 1 -eq 2" "one is not two"; }' \
  'test_fails_status() { assert_status_code 0 "echo out; echo err >&2; exit 4"; }' \
  'test_fails_fail() { fail "plainly"; }' > tests/test_own.sh
bash ./bash_unit -s skipped_by tests/test_own.sh < /dev/null 2>&1; echo "own: $?"
bash ./bash_unit -p equals -p status tests/test_own.sh < /dev/null 2>&1; echo "pattern: $?"
bash ./bash_unit -f tap -p fake -p fails_fail tests/test_own.sh < /dev/null 2>&1; echo "tap: $?"
