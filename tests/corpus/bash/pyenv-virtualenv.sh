# source: https://github.com/pyenv/pyenv-virtualenv/tree/7d821a6 (`bin`, `libexec`, `etc`, `shims`, `test` and LICENSE, unchanged, as the fixture directory `pyenv-virtualenv`), run by the bats-core fixture
# desc: pyenv-virtualenv, pyenv's plugin for virtual environments: the files of its bats suite that pass under Git Bash on Windows, activate and deactivate for bash, zsh and fish with and without a prompt, with the environment's name, from the version in use and from an argument, in a directory with a `.python-version`, the environments listed with and without their versions, pip's variables, and the version with virtualenv and with venv; the plugin's commands are stubbed as its suite stubs them, through copies of its stub script
# timeout: 600
# unixpaths: bats wants a test's directory to start with `/`, as Git Bash's does; Bash
# has no such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
export HOME="$PWD/home"; mkdir -p "$HOME"
export TMPDIR="$PWD/tmp"; mkdir -p "$TMPDIR"
here=$PWD; windows=$(pwd -W)
hide() { sed -e "s|$here|WORK|g" -e "s|$windows|WORK|g"; }
cd pyenv-virtualenv || exit 1
# The suite points TMP, which Windows' own temporary files go by, at test/tmp, a
# directory its repository keeps empty; it is made here, since an empty directory is
# not kept in this one.
mkdir -p test/tmp
# Left out: the conda, delete, hooks, init, installer, prefix, python, pyvenv,
# virtualenv and virtualenvs files, where a symbolic link, a shell found through `ps`,
# a `python` that is an empty file, or a path printed in a failure differs on Windows.
bash ../bats-core/bin/bats --formatter tap test/activate.bats test/deactivate.bats test/envs.bats test/pip.bats test/version.bats | hide
echo "status: ${PIPESTATUS[0]}"
