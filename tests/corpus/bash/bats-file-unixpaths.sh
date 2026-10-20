# source: https://github.com/bats-core/bats-file/tree/v0.4.0 (`load.bash`, `src` and LICENSE, unchanged, as the fixture directory `bats-file`), with the bats-support fixture, run by the bats-core fixture
# desc: bats-file's assertions under bats-core: files, directories, devices, links and fifos that exist or not, sizes, contents by regular expression, empty files, equal files, a temporary directory made and deleted; and failures reported in bats-support's two-column format; under cash's unixpaths, where bats' `load` takes the libraries' paths, which start with `/`, for absolute
# timeout: 90
# unixpaths: $PWD is /c/..., as in Git Bash, so this runs without the C:/ trouble of
# the case it twins; Bash has no such option, and says so on standard error.
shopt -s unixpaths 2>/dev/null
mkdir -p files/dir
printf 'alpha\nbeta\n' > files/a.txt
printf 'alpha\nbeta\n' > files/b.txt
printf 'gamma\n' > files/c.txt
: > files/empty.txt
printf '%s\n' \
  'setup() { load "bats-support/load"; load "bats-file/load"; }' \
  '@test "exists" { assert_exists files/a.txt; assert_file_exists files/a.txt; assert_dir_exists files/dir; assert_character_exists /dev/null; }' \
  '@test "does not exist" { assert_not_exists files/none; assert_file_not_exists files/dir; assert_dir_not_exists files/a.txt; assert_link_not_exists files/a.txt; assert_fifo_not_exists files/a.txt; assert_socket_not_exists files/a.txt; assert_block_not_exists files/a.txt; }' \
  '@test "sizes" { assert_size_zero files/empty.txt; assert_size_not_zero files/a.txt; assert_file_size_equals files/c.txt 6; }' \
  '@test "contents" { assert_file_contains files/a.txt beta; assert_file_contains files/a.txt "^al.*a$"; assert_file_not_contains files/a.txt delta; assert_file_empty files/empty.txt; assert_file_not_empty files/c.txt; }' \
  '@test "equal files" { assert_files_equal files/a.txt files/b.txt; assert_files_not_equal files/a.txt files/c.txt; }' \
  '@test "temp dir" { d=$(temp_make); assert_dir_exists "$d"; temp_del "$d"; assert_dir_not_exists "$d"; }' \
  '@test "temp dir with a prefix" { d=$(temp_make --prefix mine); [[ ${d##*/} == mine* ]]; temp_del "$d"; }' \
  '@test "fails: file exists" { assert_file_exists files/none.txt; }' \
  '@test "fails: dir not exists" { assert_dir_not_exists files/dir; }' \
  '@test "fails: size zero" { assert_size_zero files/c.txt; }' \
  '@test "fails: file contains" { assert_file_contains files/c.txt alpha; }' \
  '@test "fails: file empty" { assert_file_empty files/c.txt; }' \
  '@test "fails: files equal" { assert_files_equal files/a.txt files/c.txt; }' > t.bats
bash bats-core/bin/bats --formatter tap t.bats; echo "status: $?"
