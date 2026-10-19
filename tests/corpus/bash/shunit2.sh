# source: https://github.com/kward/shunit2/blob/master/shunit2 (the fixture `shunit2`, unchanged)
# desc: a real test framework run: its tests found among the functions, setUp before each, assertEquals, assertNotEquals, assertTrue, assertNull, a failing assertion's message, the summary and its status
printf '%s\n' \
  'setUp() { state=ready; }' \
  'testAddition() { assertEquals 4 $((2 + 2)); }' \
  'testStrings() { assertEquals abc "$(printf abc)"; assertNotEquals a b; }' \
  'testTrue() { assertTrue "[ -n x ]"; }' \
  'testNull() { assertNull ""; assertNotNull x; }' \
  'testSetUp() { assertEquals ready "$state"; }' \
  'SHUNIT_COLOR=none' \
  '. ./shunit2' > passing.sh
bash ./passing.sh; echo "passing: $?"
printf '%s\n' \
  'testFails() { assertEquals "the sum" 5 $((2 + 2)); }' \
  'testPasses() { assertTrue true; }' \
  'SHUNIT_COLOR=none' \
  '. ./shunit2' > failing.sh
bash ./failing.sh; echo "failing: $?"
