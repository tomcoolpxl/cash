# source: https://github.com/nvm-sh/nvm/blob/master/nvm.sh (the fixture `nvm.sh`, unchanged, as nvm-helpers.sh has it)
# desc: nvm as a whole, offline, a 4,500-line script of functions: three Node versions put in by hand, nvm ls, aliases made, listed and removed, which, version, use and current, deactivate, and the errors for a version not there; the paths it prints written NVM, and the system Node's version left out
# timeout: 60
# nvm looks for package.json or node_modules from $PWD up with ${path_%/*} until the
# path is empty, which in cash, where $PWD is C:/..., it never is (the differences page):
# a package.json at the top ends every search there.
echo "{}" > package.json
export NVM_DIR="$PWD/nvmdir"
for v in v18.20.4 v20.17.0 v22.9.0; do
  mkdir -p "$NVM_DIR/versions/node/$v/bin"
  printf '%s\n' '#!/bin/sh' "echo $v" > "$NVM_DIR/versions/node/$v/bin/node"
  chmod +x "$NVM_DIR/versions/node/$v/bin/node"
done
. ./nvm.sh --no-use
n() { nvm "$@" 2>&1 | sed -e "s|$NVM_DIR|NVM|g" -e 's|system \* (-> v[0-9.]*)|system *|'; echo "status: ${PIPESTATUS[0]}"; }
n --version | sed 's/^[0-9][0-9.]*$/VERSION/'
n ls --no-colors
n alias default 20
n alias lts-ish 18.20
n alias --no-colors
n ls --no-colors 22
n which 18
n version 20
n version 99
n use --silent 22
n current
n deactivate --silent
n current
n unalias lts-ish
n alias --no-colors
n which 99
n use 99
