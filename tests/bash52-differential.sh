#!/usr/bin/env bash
set -uo pipefail

bash52=${1:-bash}
cash=${2:-target/debug/cash}

if [[ $($bash52 --noprofile --norc -c 'printf %s "$BASH_VERSION"') != 5.2.* ]]; then
    printf 'expected a Bash 5.2 oracle, got: ' >&2
    "$bash52" --version | head -n 1 >&2
    exit 2
fi

scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
export HOME="$scratch/home"
mkdir -p "$HOME"

failures=0
run_case() {
    local name=$1 script=$2 bash_out bash_status cash_out cash_status
    bash_out=$($bash52 --noprofile --norc -c "$script" 2>/dev/null)
    bash_status=$?
    cash_out=$($cash --noprofile --norc -c "$script" 2>/dev/null)
    cash_status=$?
    if [[ $bash_status != "$cash_status" || $bash_out != "$cash_out" ]]; then
        printf 'FAIL %s\n  bash[%s]=<%s>\n  cash[%s]=<%s>\n' \
            "$name" "$bash_status" "$bash_out" "$cash_status" "$cash_out" >&2
        failures=$((failures + 1))
    else
        printf 'PASS %s\n' "$name"
    fi
}

run_case heredoc-dollar-quotes $'cat <<EOF\n$\'one\\ntwo\'\nEOF'
run_case invalid-transform 'v=x; echo before; echo ${v@}; echo after'
run_case nameref-array 'set -u; v=(one two); declare -n r="v[@]"; printf "<%s>\n" "$r"'
run_case unset-associative 'declare -A a; key="literal key"; a[$key]=x; unset '\''a[$key]'\''; echo ${#a[@]}'
run_case subscript-once 'i=0; a=(x); test -v '\''a[i++]'\''; printf "%s:%s\n" "$?" "$i"'
run_case ulimit-trailing 'limit=$(ulimit -Sn); ulimit -n -S "$limit"; echo $?'
run_case command-p-hash 'hash -p /definitely/missing echo; command -p -v echo'
run_case posix-printf-long-double 'set -o posix; printf "%.2Lf\n" 1.25'
run_case posix-command-substitution-alias 'shopt -u expand_aliases; set -o posix; alias hi="echo ok"; eval '\''echo $(hi)'\''; set +o posix; shopt -q expand_aliases; echo restored=$?'
run_case globstar 'mkdir -p "$HOME/g/a/b"; : >"$HOME/g/a/needle"; : >"$HOME/g/a/b/needle"; cd "$HOME/g"; shopt -s globstar; printf "<%s>\n" **/needle'
run_case variable-fd 'f="$HOME/fd"; : {fd}>"$f"; printf x >&$fd; {fd}>&-; cat "$f"'
run_case varredir-close 'f="$HOME/auto"; shopt -s varredir_close; : {fd}>"$f"; printf x >&$fd 2>/dev/null; echo $?'

if (( failures != 0 )); then
    printf '%s Bash 5.2 differential case(s) failed\n' "$failures" >&2
    exit 1
fi
