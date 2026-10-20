# source: https://github.com/Trepan-Debuggers/bashdb/tree/5.2-1.2.0 (`bashdb.in`, `bashdb-main.inc.in`, `bashdb-part2.sh`, `dbg-main.sh`, `getopts_long.sh`, `set-d-vars.sh` and the scripts of `init`, `lib`, `command` and `data`, unchanged, with its COPYING, as the fixture directory `bashdb`); the case fills in the two `.in` templates as `configure` would
# desc: the Bash debugger driving a script from a command file: listing, step and next into and over functions, info args, print, a backtrace through source() and main(), finish, display, a breakpoint, continue past the end, eval, and the debugger's own messages when the program has gone; under cash's unixpaths, where bashdb's files start with `/`, which it takes for absolute
# timeout: 60
# unixpaths: $PWD is /c/..., as in Git Bash, so this runs without the C:/ trouble of
# the case it twins; Bash has no such option, and says so on standard error.
shopt -s unixpaths 2>/dev/null
cd bashdb || exit 1
L=$PWD
for f in bashdb.in bashdb-main.inc.in; do
  sed -e "s|@PACKAGE@|bashdb|g" -e "s|@PACKAGE_VERSION@|5.2-1.2.0|g" \
    -e "s|@PKGDATADIR@|$L|g" -e "s|@prefix@|$L|g" -e "s|@SH_PROG@|bash|g" \
    -e "s|@BASHDB_MAIN@|$L/bashdb-main.inc|g" -e "s|@DBGR_MAIN@|$L/bashdb-main.inc|g" \
    "$f" > "${f%.in}"
done
printf '%s\n' 'x=1' 'f() {' '  local y=$((x + 1))' '  echo "y=$y in $1"' '  g "$y"' '}' 'g() {' \
  '  echo "g got $1"' '}' 'f arg1' 'for i in 1 2; do' '  echo "i=$i"' 'done' 'echo done' > ex.sh
printf '%s\n' 'list 1,5' 'step' 'next' 'step' 'info args' 'print $1' 'step' 'step' 'backtrace' \
  'finish' 'display $i' 'break 12' 'continue' 'continue' 'eval x=5; echo "x=$x"' 'info display' \
  'delete 1' 'continue' 'quit' > cmds
bash ./bashdb -q -x cmds ex.sh < /dev/null 2>&1 | sed -e "s|$L|W|g" -e "s|$(pwd -W)|W|g"
echo "status: ${PIPESTATUS[0]}"
