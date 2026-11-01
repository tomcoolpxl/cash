# source: https://github.com/bats-core/bats-detik/tree/2ea9f37 (`lib`, `load.bash`, the six `tests/*.bats` named below, `tests/resources` and LICENSE, unchanged, as the fixture directory `bats-detik`), run by the bats-core fixture
# desc: bats-detik at 2ea9f37, the Kubernetes assertions for bats: its utils (trim, to_lower_case, debug), the building of a kubectl request, the `verify` assertions against a client the tests stand in for, and the linter of DETIK test files, by bats-core's TAP formatter; the `try` suites, which wait between their attempts, are left out for time
# timeout: 600
# unixpaths: bats wants a test's directory to start with `/`, as Git Bash's does; Bash
# has no such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
export TMPDIR="$PWD/tmp"; mkdir -p "$TMPDIR"
cd bats-detik || exit 1
for suite in test.utils.trim test.utils.to_lower_case test.utils.debug test.detik.build_k8s_request test.detik.verify test.linter; do
  echo "== $suite"
  bash ../bats-core/bin/bats --formatter tap "tests/$suite.bats"
done
