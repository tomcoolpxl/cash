# source: https://github.com/direnv/direnv/tree/v2.38.2 (`stdlib.sh`, unchanged, with its LICENSE, as the fixture directory `direnv`)
# desc: direnv's stdlib, the functions an .envrc calls, with a function standing in for the direnv program it calls back: has, join_args, expand_path, user_rel_path, PATH_add, path_add and PATH_rm on colon-separated lists, MANPATH_add with and without a MANPATH, find_up finding a file above, source_env, semver_search, log_status and log_error, strict_env and unstrict_env, and the layout directory
# timeout: 30
top=$PWD
# direnv itself is a Go program; its stdlib calls it back, so a function stands for it.
direnv() {
  case $1 in
    log) shift; echo "direnv: ${*//$top/TOP}" ;;
    *) echo "direnv called: ${*//$top/TOP}" ;;
  esac
}
export HOME="$PWD/home"
mkdir -p home proj/sub/deep proj/bin "home/.local/share/man" versions/node-18.20.1 versions/node-20.11.0 versions/node-20.12.2
show() { local v=${!1}; echo "$1=${v//$top/TOP}"; }
. direnv/stdlib.sh
echo "has: $(has bash && echo yes) $(has no-such-tool || echo no)"
echo "join_args: $(join_args a 'b c' "d'e")"
v=$(expand_path proj/sub); echo "expand_path: ${v//$top/TOP}"
v=$(expand_path ../x "$top/proj/sub"); echo "expand_path from: ${v//$top/TOP}"
echo "user_rel_path: $(user_rel_path "$HOME/notes/a.txt") $(user_rel_path /elsewhere/b.txt)"
PATH_before=$PATH
PATH=/usr/bin:/bin
PATH_add proj/bin
show PATH
path_add MYPATH proj/sub one/two
show MYPATH
PATH_rm "*proj*"
show PATH
path_rm MYPATH "*two"
show MYPATH
PATH=$PATH_before
unset MANPATH
MANPATH_add home/.local/share/man
show MANPATH
MANPATH=/usr/share/man
MANPATH_add home/.local/share/man
show MANPATH
printf 'export FROM_ENVRC=yes\n' > proj/.envrc
touch proj/marker
cd proj/sub/deep || exit 1
v=$(find_up marker); echo "find_up: ${v//$top/TOP}"
source_env "$top/proj/.envrc" 2>&1
echo "FROM_ENVRC=$FROM_ENVRC"
cd "$top" || exit 1
echo "semver_search 20: $(semver_search versions node- 20)"
echo "semver_search 20.11: $(semver_search versions node- 20.11)"
echo "semver_search 18: $(semver_search versions node- 18)"
log_status "loading done" 2>&1
log_error "an error" 2>&1
strict_env
echo "strict: $- $(set -o | grep pipefail | tr -s ' \t' ' ')"
unstrict_env
echo "unstrict: $-"
v=$(direnv_layout_dir); echo "layout dir: ${v//$top/TOP}"
