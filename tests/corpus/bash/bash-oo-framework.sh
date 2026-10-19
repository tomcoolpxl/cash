# source: https://github.com/niieani/bash-oo-framework/tree/0a9ea03 (its `lib` and the examples of `example`, unchanged, as the fixture directory `bash-oo-framework`)
# desc: an object framework for bash: import of modules by path, types whose methods are found with compgen -A function and rewritten from declare -f with eval, function names such as `boolean.=`, strings with methods, try and catch made of aliases that close a `{ }` and a subshell, an ERR trap that `exit` does not fire; its array example and the test DSL's are left out, the one slow and the other drawing with tput
# timeout: 60
for example in string human exitcode; do
  echo "== $example"
  bash bash-oo-framework/example/$example.sh 2>&1; echo "status: $?"
done
echo "== try and catch"
printf '%s\n' \
  'source "$PWD/bash-oo-framework/lib/oo-bootstrap.sh"' \
  'import util/tryCatch' \
  'try { echo "in try"; false; echo "not reached"; } catch { echo "caught, status $__oo__tryResult"; }' \
  'try { echo "nothing fails"; } catch { echo "not caught"; }' \
  'echo "after both"' > trycatch-case.sh
bash trycatch-case.sh 2>&1; echo "status: $?"
