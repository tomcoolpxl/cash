# source: cash's own, from Bash 5.3's run_debug_trap calls in execute_cmd.c, as bats' failure traces need them
# desc: where the DEBUG trap runs and what it sees: before [[ ]] and (( )), each turn of a for loop and each part of an arithmetic for, once before a case and a select, before each simple stage of a pipeline in the shell itself, at a function's start under set -T or declare -ft on its body's line; $LINENO, $BASH_COMMAND as Bash spells each, FUNCNAME; PIPESTATUS and an ignored failure kept across the trap
trap 'printf "[%s|%s|%s]\n" "$LINENO" "$BASH_COMMAND" "${FUNCNAME[*]}"' DEBUG
[[   a   ==   a  &&  -n x ]]
(( 1 + 2 ))
for i in 1 "2 3"; do :; done
for i in; do :; done
set -- p q
for i; do :; done
for (( j = 0 ; j < 2 ; j++ )); do :; done
for ((;;)); do break; done
v=x
case "$v" in x) : ;; esac
select s in a b; do break; done <<< 1 2>/dev/null
echo a \
  | cat >/dev/null \
  | true
{ :; } | cat
if [[ -n x ]]; then :; fi
while false; do :; done
f() {
  [[ 1 == 1 ]]
}
f
set -T
f
g()
{
  :
}
g
echo a | cat >/dev/null
( : )
set +T
k() { [[ 1 == 1 ]]; }
k
declare -ft k
k
trap - DEBUG
trap 'x=1; false | true; [[ a == b ]] && :' DEBUG
false | true | true
echo "kept: ${PIPESTATUS[*]}"
set -e
h() {
  local -i k
  for (( k = 0; k < 2; ++k )); do
    [[ $k == 9 ]] && return 0
  done
  echo "h went on"
}
h
echo end
