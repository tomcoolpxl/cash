# source: https://github.com/rbenv/rbenv/tree/v1.3.2/libexec (the fixture directory `rbenv`, unchanged)
# desc: a real program of many scripts: its dispatcher puts its own directory on PATH and execs each command as rbenv-NAME, the version from RBENV_VERSION or the nearest .ruby-version up the tree, versions --bare, version-origin, global and local writing their files, local --unset, an unknown command and an unknown version
export RBENV_ROOT="$PWD/root" HOME="$PWD/home"
unset RBENV_VERSION RBENV_DIR
mkdir -p root/versions/2.7.8 root/versions/3.3.1 home project/sub
root=$PWD
# A .ruby-version at the top: rbenv looks for one up the tree with ${root%/*} until the
# path is empty, and cash's $PWD, C:/..., ends at C:, where that loop never ends.
printf '2.7.8\n' > .ruby-version
rb="$root/rbenv/rbenv"
r() { "$rb" "$@"; }
r --version
r root | sed "s|$root/||"
r versions --bare
r version-name
r version-origin | sed "s|$root/||"
r global 3.3.1; echo "global: $?"
r global
cat root/version
cd project/sub || exit 1
r local 3.3.1; echo "local: $?"
cat .ruby-version
r version | sed "s|$root/||"
cd .. || exit 1
r version | sed "s|$root/||"
r version-file | sed "s|$root/||"
cd sub || exit 1
r version-file | sed "s|$root/||"
RBENV_VERSION=2.7.8 r version | sed "s|$root/||"
r local 9.9.9 2>&1; echo "unknown version: $?"
r nosuchcommand 2>&1; echo "unknown command: $?"
r local --unset; echo "unset: $?"
ls -a | grep -c ruby-version
r version-name
r commands --no-sh | grep -c .
