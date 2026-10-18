# source: https://github.com/dylanaraps/pure-bash-bible#traps
# desc: EXIT on the script and a subshell, '' INT shown by trap -p, DEBUG with set -T, RETURN on a function and a sourced file
trap 'printf "exit trap\n"' EXIT
( trap 'echo subshell exit' EXIT; echo in subshell )
trap '' INT
trap -p INT
trap - INT
trap -p INT
n=0
trap '((n++))' DEBUG
: one
: two
trap - DEBUG
echo "debug ran $n times"
f() { echo "in f"; }
trap 'echo "returned from ${FUNCNAME[0]:-a file}"' RETURN
f
set -o functrace
f
set +o functrace
printf 'echo sourced\n' >lib.sh
. ./lib.sh
trap - RETURN
f
echo end
