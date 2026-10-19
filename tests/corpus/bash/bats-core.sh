# source: https://github.com/bats-core/bats-core/tree/v1.14.0 (bin, libexec and lib as the fixture directory `bats-core`, unchanged)
# desc: a real test runner of many scripts: a .bats file preprocessed into bash, each test run in its own process, run capturing status and output, setup and teardown, skip, a failing test's report with its file and line, the TAP and pretty-free formatters, --count, a test filter
# timeout: 90
printf '%s\n' \
  'setup() { greeting=hello; }' \
  'teardown() { :; }' \
  '@test "addition" {' \
  '  result="$((2 + 2))"' \
  '  [ "$result" -eq 4 ]' \
  '}' \
  '@test "run captures output and status" {' \
  '  run printf "%s\n" one two' \
  '  [ "$status" -eq 0 ]' \
  '  [ "${lines[1]}" = two ]' \
  '  [ "$output" = "one' \
  'two" ]' \
  '}' \
  '@test "setup ran" {' \
  '  [ "$greeting" = hello ]' \
  '}' \
  '@test "skipped" {' \
  '  skip "not today"' \
  '}' \
  '@test "failing" {' \
  '  run false' \
  '  [ "$status" -eq 0 ]' \
  '}' > t.bats
bats=bats-core/bin/bats
bash "$bats" --count t.bats
bash "$bats" --formatter tap t.bats; echo "status: $?"
bash "$bats" --formatter tap --filter 'add|setup' t.bats; echo "filtered: $?"
bash "$bats" --version
