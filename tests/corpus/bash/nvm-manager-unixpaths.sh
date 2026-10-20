# source: https://github.com/nvm-sh/nvm/blob/master/nvm.sh (the fixture `nvm.sh`, unchanged, as nvm-helpers.sh has it)
# desc: nvm as a whole, offline, a 4,500-line script of functions: three Node versions put in by hand, nvm ls, aliases made, listed and removed, which, version, use and current, deactivate, and the errors for a version not there; the paths it prints written NVM, and the system Node's version left out; under cash's unixpaths, with no package.json at the top: the search up the tree ends at the root
# timeout: 150
# unixpaths: $PWD is /c/..., as in Git Bash, so this runs without the C:/ workaround of
# the case it twins; Bash has no such option, and says so on standard error.
shopt -s unixpaths 2>/dev/null
# GitHub's Windows runners set npm_config_prefix, which nvm use refuses with status 11.
unset npm_config_prefix NPM_CONFIG_PREFIX
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
