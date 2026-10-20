# source: cash's own, from Bash 5.3's run_debug_trap and execute_cmd.c under extdebug
# desc: shopt -s extdebug: a DEBUG trap that fails skips the command, a simple one, [[ ]], (( )), a case, a pipeline's stage (with no place in PIPESTATUS), a for loop's turn (the loop's status the trap's), an arithmetic for's condition (taken for 0) and a function's body (with the trap's status); a trap ending with 2 in a function returns from it
shopt -s extdebug
trap '[[ $BASH_COMMAND != *SKIP* ]]' DEBUG
echo SKIP-simple
echo "kept $?"
[[ SKIP == x ]] && echo "cond true"
echo "after cond $?"
(( SKIP = 5 ))
echo "arith SKIP=${SKIP-unset} $?"
case SKIP in *) echo "case ran" ;; esac
echo "after case $?"
echo SKIP | cat
echo "ps ${PIPESTATUS[*]}"
trap - DEBUG
trap '[[ $BASH_COMMAND != "for i in 1 2" ]]' DEBUG
for i in 1 2; do echo "turn $i"; done
echo "after for $?"
trap - DEBUG
trap '[[ $BASH_COMMAND != "((j < 2))" ]]' DEBUG
for (( j = 0; j < 2; j++ )); do echo "j $j"; done
echo "after arith for j=$j"
trap - DEBUG
f() { echo in-f; }
set -T
c=0
trap 'c=$((c+1)); (( c != 2 ))' DEBUG
f
echo "f status $?"
trap - DEBUG
g() { echo before; echo SKIP2; echo after; }
trap 'if [[ $BASH_COMMAND == *SKIP2* ]]; then (exit 2); fi' DEBUG
g
echo "g status $?"
trap - DEBUG
echo end
