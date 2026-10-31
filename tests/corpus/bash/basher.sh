# source: https://github.com/basherpm/basher/tree/7c3d238 (`bin`, `libexec`, `lib`, `completions`, `tests` and LICENSE, unchanged, as the fixture directory `basher`; the `tests/vendor` submodules are the bats-support and bats-assert fixtures), run by the bats-core fixture
# desc: basher, the package manager for shell scripts: its own bats suite, the files that need no symbolic link, with packages cloned from local repositories through its clone mock, the dispatcher, commands, dependencies, completions, help, the init script for bash, zsh and dash, install, list, outdated, upgrade, uninstall and package-path
# timeout: 900
# unixpaths: bats wants a test's directory to start with `/`, as Git Bash's does; Bash
# has no such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
export HOME="$PWD/home"; mkdir -p "$HOME"
export GIT_AUTHOR_NAME=A GIT_AUTHOR_EMAIL=a@example.com GIT_COMMITTER_NAME=A GIT_COMMITTER_EMAIL=a@example.com
export TMPDIR="$PWD/tmp"; mkdir -p "$TMPDIR"
mkdir -p basher/tests/vendor && cp -r bats-support basher/tests/vendor/bats-core && cp -r bats-assert basher/tests/vendor/bats-assert
cd basher || exit 1
# Left out: the link and unlink files and basher-link, whose `ln -s` Windows makes a copy
# of, so `readlink` finds no link in either shell; and the verbose listing, whose clone
# URL is the Windows spelling of a path the test expects in Unix spelling.
bash ../bats-core/bin/bats --formatter tap tests/basher.bats tests/basher-_clone.bats tests/basher-_commands.bats \
  tests/basher-_deps.bats tests/basher-completions.bats tests/basher-help.bats \
  tests/basher-init.bats tests/basher-install.bats tests/basher-outdated.bats tests/basher-package-path.bats \
  tests/basher-uninstall.bats tests/basher-upgrade.bats tests/include.bats; echo "status: $?"
bash ../bats-core/bin/bats --formatter tap --filter 'usage|installed packages' tests/basher-list.bats; echo "list: $?"
