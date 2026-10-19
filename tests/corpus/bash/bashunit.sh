# source: https://github.com/TypedDevs/bashunit/tree/0.52.0 (the runner `bashunit` and its `src` as the fixture directory `bashunit`, unchanged)
# desc: a real test framework of many sourced files, under set -euo pipefail: test functions found by name, set_up before each, assertions passing and one failing with its message, the summary and its status, a filter, simple output; the times each test took are left out
# timeout: 60
export BASHUNIT_NO_NETWORK=true NO_COLOR=1; unset GITHUB_ACTIONS
printf '%s\n' \
  'function set_up() { greeting=hello; }' \
  'function test_addition() { assert_equals 4 "$((2 + 2))"; }' \
  'function test_contains() { assert_contains "world" "hello world"; }' \
  'function test_set_up_ran() { assert_equals hello "$greeting"; }' \
  'function test_empty() { assert_empty ""; assert_not_empty x; }' \
  'function test_fails() { assert_equals "expected" "actual"; }' > example_test.sh
bash ./bashunit/bashunit test example_test.sh --no-color 2>&1 | grep -v -e 'Time taken' -e '^bashunit - ' | sed 's/  *[0-9][0-9]*ms$//'; echo "status: ${PIPESTATUS[0]}"
bash ./bashunit/bashunit test example_test.sh --no-color --filter addition 2>&1 | grep -v -e 'Time taken' -e '^bashunit - ' | sed 's/  *[0-9][0-9]*ms$//'; echo "filtered: ${PIPESTATUS[0]}"
bash ./bashunit/bashunit test example_test.sh --no-color --simple 2>&1 | grep -v -e 'Time taken' -e '^bashunit - ' | sed 's/  *[0-9][0-9]*ms$//'; echo "simple: ${PIPESTATUS[0]}"
