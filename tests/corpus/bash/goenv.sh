# source: https://github.com/go-nv/goenv/tree/d8c9cd6 (`libexec`, `completions`, `test`, `plugins/go-build/bin`, APP_VERSION and LICENSE, unchanged, as the fixture directory `goenv`; `bin/goenv`, a link to `libexec/goenv` upstream, is left out), run by the bats-core fixture
# desc: goenv, the Go version manager in the rbenv family: its bats suite but the version test, whose top-level code reads a file through bats' own path; the commands, completions, exec, global, help, hooks, init, installed, local, prefix, rehash, root, the shell integration, shims, the version files read and written, version-name, version-origin, version, versions, whence and which, with the tests that need a symbolic link, a shell found through `ps`, a read-only directory or a `go` of the machine failing the same way in both shells; the suite's helpers hide the test directory's path
# timeout: 1200
# unixpaths: bats wants a test's directory to start with `/`, as Git Bash's does; Bash
# has no such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
export HOME="$PWD/home"; mkdir -p "$HOME"
export TMPDIR="$PWD/tmp"; mkdir -p "$TMPDIR"
# bats names its own path in the warnings it ends with, for a `run` whose command was
# not found; the directory's path is hidden in both spellings.
# goenv finds its APP_VERSION by cutting its path at the first `goenv` in it, which the
# runner's directory holds, so one test names a path under the runner's root that is
# not there; the root is hidden in both spellings.
here=$PWD; windows=$(pwd -W)
root=${here%/*}; root=${root%/*}; windows_root=${windows%/*}; windows_root=${windows_root%/*}
hide() { sed -e "s|$root|CORPUS|g" -e "s|$windows_root|CORPUS|g"; }
cd goenv || exit 1
files=$(ls test/*.bats | grep -v 'goenv--version\|goenv-init\|goenv-hooks\|goenv-versions\|goenv_ext')
# shellcheck disable=SC2086
bash ../bats-core/bin/bats --formatter tap $files | hide
echo "status: ${PIPESTATUS[0]}"
# Left out of these two: the aliases skipped, which are symbolic links upstream, a copy
# in Git Bash and a directory link in cash; and a file reached through a symbolic link,
# which Windows makes a copy, cash saying so.
bash ../bats-core/bin/bats --formatter tap --filter 'usage|completion|unknown|system|warning|bare|showing|single|GOENV_VERSION|globally|local|non-directories|prints symlinks' test/goenv-versions.bats | hide
echo "versions: ${PIPESTATUS[0]}"
bash ../bats-core/bin/bats --formatter tap --filter 'prefixes|dirname' test/goenv_ext.bats | hide
echo "ext: ${PIPESTATUS[0]}"
# Left out of these two: the parent shell found through `ps -p`, which neither shell's
# ps answers, so that the fallback is `$SHELL`, a different name in each; and a hook
# reached through a symbolic link, which Windows makes a copy, cash saying so.
bash ../bats-core/bin/bats --formatter tap --filter 'usage|completion|does not create|snippet|creates shims|rehash|bootstrap|automatic|collision|warning' test/goenv-init.bats | hide
echo "init: ${PIPESTATUS[0]}"
bash ../bats-core/bin/bats --formatter tap --filter 'completion|usage|ending|spaces|relative' test/goenv-hooks.bats | hide
echo "hooks: ${PIPESTATUS[0]}"
