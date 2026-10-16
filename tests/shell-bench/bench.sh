#!/usr/bin/env bash
# The shell benchmarks: the same bash workloads in each shell given, timed by hyperfine
# (`scoop install hyperfine`, `pacman -S hyperfine`), one JSON file per workload in OUTDIR.
# run.sh runs it for cash, Git Bash and WSL's bash and prints the table.
#
#   bash tests/shell-bench/bench.sh OUTDIR NAME=SHELL...
#
# Each SHELL runs `-c SCRIPT`. The workloads are those of crates/cash-shell/benches
# (start, loop, echo, arithmetic, expansion, function call), and what costs a process:
# a command substitution, a program (hostname.exe on Windows, /bin/true on Linux), and
# a pipe into cat.
set -u
out=$1
shift
mkdir -p "$out"
case $(uname -s) in
    Linux) prog=/bin/true ;;
    *) prog='C:/Windows/System32/HOSTNAME.EXE >/dev/null' ;;
esac
shells=("$@")

bench() {
    local name=$1 script=$2 args=() shell
    for shell in "${shells[@]}"; do
        args+=(-n "${shell%%=*}" "\"${shell#*=}\" -c '$script'")
    done
    if hyperfine -N --warmup 2 --min-runs 10 --export-json "$out/$name.json" "${args[@]}" \
        > "$out/$name.txt" 2>&1; then
        echo "done: $name"
    else
        echo "FAILED: $name (see $out/$name.txt)"
    fi
}

bench startup 'true'
bench loop 'for ((i=0;i<20000;i++)); do :; done'
bench echo 'for ((i=0;i<20000;i++)); do echo hi; done >/dev/null'
bench arithmetic 'for ((i=0;i<20000;i++)); do x=$((i*3+i/2%7)); done'
bench expansion 's=abcdef; for ((i=0;i<20000;i++)); do x=${s/b/B}; y=${s:2:3}; z=${#s}; done'
bench function 'f() { :; }; for ((i=0;i<20000;i++)); do f; done'
bench substitution 'for ((i=0;i<1000;i++)); do x=$(echo hi); done'
bench program "for ((i=0;i<100;i++)); do $prog; done"
bench pipe 'for ((i=0;i<100;i++)); do echo hi | cat >/dev/null; done'
