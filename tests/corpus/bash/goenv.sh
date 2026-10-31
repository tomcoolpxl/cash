# source: https://github.com/go-nv/goenv/tree/d8c9cd6 (`libexec`, `completions`, `test`, `plugins/go-build/bin`, APP_VERSION and LICENSE, unchanged, as the fixture directory `goenv`; `bin/goenv`, a link to `libexec/goenv` upstream, is left out), run by the bats-core fixture
# desc: goenv, the Go version manager in the rbenv family: ten files of its bats suite, two minutes of its eight under cash, the dispatcher (goenv.bats), commands, completions, help, rehash, shims, the version file read, version-name, version-origin, and init without its parent-shell detection; left out for time: exec, global, local, prefix, installed, the shell integration, version, versions, whence and which, and for Windows the version test, whose top-level code reads a file through bats' own path, and the tests that reach a hook, an alias or a file through a symbolic link; a `go` of the machine fails a test the same way in both shells, and the suite's helpers hide the test directory's path
# timeout: 900
# unixpaths: bats wants a test's directory to start with `/`, as Git Bash's does; Bash
# has no such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
export HOME="$PWD/home"; mkdir -p "$HOME"
export TMPDIR="$PWD/tmp"; mkdir -p "$TMPDIR"
# bats names its own path in the warnings it ends with, for a `run` whose command was
# not found; the directory's path is hidden in both spellings.
# The directory's path, should a test name it, is hidden in both spellings.
here=$PWD; windows=$(pwd -W)
hide() { sed -e "s|$here|WORK|g" -e "s|$windows|WORK|g"; }
cd goenv || exit 1
bash ../bats-core/bin/bats --formatter tap test/goenv-commands.bats test/goenv-completions.bats test/goenv-help.bats test/goenv-rehash.bats test/goenv-shims.bats test/goenv-version-file-read.bats test/goenv-version-name.bats test/goenv-version-origin.bats | hide
echo "status: ${PIPESTATUS[0]}"
# Left out of the dispatcher's file: the help printed with no argument, which starts
# with the version, read from a path goenv cuts at the first `goenv` in its own: whether
# that is the fixture directory or a directory above it depends on where the case runs.
bash ../bats-core/bin/bats --formatter tap --filter 'completion|install|invalid|GOENV_ROOT|GOENV_DIR|adds|GOENV_HOOK_PATH|shell|latest local' test/goenv.bats | hide
echo "dispatcher: ${PIPESTATUS[0]}"
# Left out of this one: the parent shell found through `ps -p`, which neither shell's
# ps answers, so that the fallback is `$SHELL`, a different name in each.
bash ../bats-core/bin/bats --formatter tap --filter 'usage|completion|does not create|snippet|creates shims|rehash|bootstrap|automatic|collision|warning' test/goenv-init.bats | hide
echo "init: ${PIPESTATUS[0]}"
