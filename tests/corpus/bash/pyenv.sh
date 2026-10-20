# source: https://github.com/pyenv/pyenv/tree/v2.8.10 (`libexec`, `plugins/python-build/bin` but `pyenv-install-prerequisites`, which runs sudo, and three of python-build's definitions, unchanged, with its LICENSE, as the fixture directory `pyenv`)
# desc: a version manager of thirty small scripts calling one another, with python-build's 2,500-line installer, offline: two versions put in by hand, listed bare and not, the global version written, the versions of the top directory and of a project below it, which, prefix and the origin of the choice, shims made and a command run through exec, the latest of a prefix, the definitions python-build knows, an uninstall and errors; paths under the case's directory written PWD
# timeout: 60
# pyenv looks for .python-version from $PWD up with root=${root%/*} until root is empty,
# which in cash, where $PWD is C:/..., it never is (the differences page): one is at the
# top, so that every search ends there. `pyenv help` is left out: it runs `$(type -P gawk
# awk)`, and Git's gawk, which cash does not carry, is under `C:/Program Files`.
export PYENV_ROOT="$PWD/pyroot" HOME="$PWD/home"
mkdir -p home proj
echo 3.12.7 > .python-version
for v in 3.12.7 3.13.0; do
  mkdir -p "$PYENV_ROOT/versions/$v/bin"
  printf '%s\n' '#!/bin/sh' "echo \"Python $v \$*\"" > "$PYENV_ROOT/versions/$v/bin/python"
  chmod +x "$PYENV_ROOT/versions/$v/bin/python"
done
top=$PWD
p() { bash "$top/pyenv/libexec/pyenv" "$@" 2>&1 | sed "s|$top|PWD|g"; echo "status: ${PIPESTATUS[0]}"; }
p --version | sed 's/pyenv [0-9][0-9.]*.*/pyenv VERSION/'
p versions
p versions --bare
p global 3.13.0
cat "$PYENV_ROOT/version"
p version
cd proj || exit 1
p version
p local 3.13.0
cat .python-version
p version
p version-name
p version-origin
p prefix
p which python
p rehash
ls "$PYENV_ROOT/shims"
p exec python --flag
p latest 3.12
p latest 3
p install --list
p local 9.9
p version-name
rm .python-version
p uninstall -f 3.13.0
p versions --bare
p frobnicate
