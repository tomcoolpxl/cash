# source: https://github.com/shellspec/shellspec/tree/0.28.1 (the runner `shellspec`, `lib` and `libexec` as the fixture directory `shellspec`, unchanged)
# desc: a real BDD test framework in POSIX sh, many files: a spec file translated into shell and run, Describe and It blocks, a function defined in the spec, When call, output and status expectations, a failing one's report, Skip, the TAP formatter, a focused example, --count
# timeout: 90
# The shell by its full path: shellspec otherwise finds it by name on PATH, which can
# be another cash or Git's sh under a directory with a space, which it splits.
shell=$(command -v sh)
mkdir -p spec tmp
: > .shellspec
export TMPDIR="$PWD/tmp"
printf '%s\n' \
  "Describe 'arithmetic'" \
  "  It 'adds'" \
  "    When call expr 2 + 2" \
  "    The output should eq 4" \
  "  End" \
  "  It 'fails on purpose'" \
  "    When call echo actual" \
  "    The output should eq expected" \
  "  End" \
  "End" \
  "Describe 'functions'" \
  "  greet() { echo \"Hello, \$1\"; }" \
  "  It 'greets'" \
  "    When call greet cash" \
  "    The output should eq 'Hello, cash'" \
  "    The status should be success" \
  "  End" \
  "  It 'skips'" \
  "    Skip 'not today'" \
  "  End" \
  "End" > spec/example_spec.sh
sh ./shellspec/shellspec --shell "$shell" --no-color --count
sh ./shellspec/shellspec --shell "$shell" --no-color --format tap 2>&1 | sed '/^# Finished in/d'; echo "status: ${PIPESTATUS[0]}"
sh ./shellspec/shellspec --shell "$shell" --no-color --format tap --example greets 2>&1 | sed '/^# Finished in/d'; echo "focused: ${PIPESTATUS[0]}"
