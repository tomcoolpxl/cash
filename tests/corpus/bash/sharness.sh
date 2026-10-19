# source: https://github.com/felipec/sharness/tree/v1.2.1 (`sharness.sh` and `lib-sharness`, unchanged, with its COPYING, as the fixture directory `sharness`)
# desc: a shell test library in POSIX sh, Git's test style: a test file sourcing it, test bodies eval'd in a trash directory, descriptors 3 and 4 for verbose output, passing, failing, known-broken and fixed tests, prerequisites set and missing, test_cmp and test_must_fail, the TAP plan and summary, verbose and immediate runs, the trash directory kept on failure
# timeout: 60
mkdir t && cd t || exit 1
printf '%s\n' '#!/bin/sh' \
  "test_description='basic sharness'" \
  '. ../sharness/sharness.sh' \
  "test_expect_success 'true passes' 'true'" \
  "test_expect_success 'a file is written in the trash' 'echo hello >out && test \"\$(cat out)\" = hello'" \
  "test_expect_failure 'a known breakage' 'false'" \
  "test_expect_failure 'a breakage fixed' 'true'" \
  "test_expect_success 'a failure' 'echo to-the-log; false'" \
  "test_expect_success MISSING 'skipped for its prerequisite' 'true'" \
  'test_set_prereq HAVE_IT' \
  "test_expect_success HAVE_IT 'run with its prerequisite' 'true'" \
  "test_expect_success 'test_cmp' 'echo a >expect && echo a >actual && test_cmp expect actual'" \
  "test_expect_success 'test_must_fail' 'test_must_fail false'" \
  'test_done' > t0001-basic.t
printf '%s\n' '#!/bin/sh' \
  "test_description='all pass'" \
  '. ../sharness/sharness.sh' \
  "test_expect_success 'one' 'true'" \
  "test_expect_success 'two' 'test 1 -lt 2'" \
  'test_done' > t0002-pass.t
sh t0001-basic.t; echo "status: $?"
ls -d trash* 2>/dev/null
rm -rf "trash directory.t0001-basic"
sh t0001-basic.t -v 2>&1; echo "verbose: $?"
rm -rf "trash directory.t0001-basic"
sh t0001-basic.t --immediate; echo "immediate: $?"
rm -rf trash*
sh t0002-pass.t; echo "status: $?"
ls -d trash* 2>/dev/null; echo "trash left: $?"
