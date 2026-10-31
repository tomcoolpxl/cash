# source: https://github.com/tj/n/tree/v11.0.0 (`bin/n`, `test/tests` and LICENSE, unchanged, as the fixture directory `n`), run by the bats-core, bats-support and bats-assert fixtures
# desc: n, the Node.js version manager, with no network: its bats tests that need none, the downloaded versions listed and the version read from .n-node-version and .node-version; then its commands on versions made by hand with a stand-in node: ls, which, bin, run, exec, rm, prune, --version and the help's first lines
# timeout: 300
# unixpaths: bats wants a test's directory to start with `/`, as Git Bash's does; Bash
# has no such option, and says so on standard error.
shopt -s unixpaths 2> /dev/null
mkdir -p n/node_modules && cp -r bats-support bats-assert n/node_modules/
export PATH="$PWD/n/bin:$PATH" TMPDIR="$PWD/tmp"; mkdir -p "$TMPDIR"
# The partial-version lookup is left out: it asks the network, and n refuses MSYS.
(cd n/test/tests && bash ../../../bats-core/bin/bats --formatter tap --filter '^n l|n-node-version|missing file|from sub directory' ls.bats version-resolve-auto-file.bats); echo "bats: $?"
export N_PREFIX="$PWD/prefix"
for v in 10.15.0 4.9.1 12.22.12; do
  mkdir -p "$N_PREFIX/n/versions/node/$v/bin"
  printf '%s\n' '#!/bin/sh' "echo \"node v$v \$*\"" > "$N_PREFIX/n/versions/node/$v/bin/node"
  chmod +x "$N_PREFIX/n/versions/node/$v/bin/node"
done
hide() { sed "s|${N_PREFIX//\\//}|PREFIX|g"; }
echo "== version"; n --version
echo "== ls"; n ls
echo "== which and bin"; n which 10.15.0 | hide; n bin 12.22.12 | hide; n which 8.0.0; echo "which 8.0.0: $?"
echo "== run and exec"; n run 10.15.0 --check app.js; n exec 4.9.1 node -e 1; n exec 12.22.12 sh -c 'echo "PATH starts at ${PATH%%:*}"' | hide
echo "== rm"; n rm 4.9.1; echo "rm: $?"; n ls; n rm 4.9.1; echo "rm again: $?"
echo "== prune"; n prune; echo "prune: $?"; n ls
echo "== help"; n --help | sed -n '2,6p'
