#!/usr/bin/env bash
# The shell benchmarks: the same workloads in each shell given, timed by hyperfine
# (`scoop install hyperfine`, `apt install hyperfine`), one JSON file per workload in
# OUTDIR. run.sh runs it for cash, Git Bash, PowerShell 7 and WSL's bash on Windows, and
# for bash on Linux, keeps the results and prints the table.
#
#   bash tests/shell-bench/bench.sh OUTDIR NAME=SHELL...
#
# A bash runs `-c SCRIPT`; PowerShell (pwsh.exe, powershell.exe) runs the workload in its
# own syntax, `-NoProfile -NonInteractive -Command SCRIPT`. The workloads are those of
# crates/cash-shell/benches (start, loop, echo, arithmetic, expansion, function call),
# what costs a process (a command substitution, a program: hostname.exe on Windows,
# /bin/true on Linux; a pipe into cat), and `ls -la` of a directory of 200 files.
# PowerShell has no process for `$(...)`, and pipes into Out-Null. BENCH_WARMUP and
# BENCH_MIN_RUNS give hyperfine other counts than 2 and 10, for a quick try.
set -u
out=$1
shift
mkdir -p "$out"
case $(uname -s) in
    Linux) prog=/bin/true ;;
    *) prog='C:/Windows/System32/HOSTNAME.EXE >/dev/null' ;;
esac
shells=("$@")

# The directory `ls -la` lists, on the system's own disk: 200 files and 10 directories.
listed=$(mktemp -d)
for i in $(seq 1 200); do printf '%s\n' "$i" > "$listed/file$i.txt"; done
for i in $(seq 1 10); do mkdir "$listed/dir$i"; done
listed_native=$listed
command -v cygpath > /dev/null && listed_native=$(cygpath -m "$listed")

bench() {
    local name=$1 script=$2 ps_script=$3 args=() shell command
    for shell in "${shells[@]}"; do
        command=${shell#*=}
        case ${command,,} in
            *pwsh*|*powershell*)
                args+=(-n "${shell%%=*}" "\"$command\" -NoProfile -NonInteractive -Command '$ps_script'") ;;
            *)
                args+=(-n "${shell%%=*}" "\"$command\" -c '$script'") ;;
        esac
    done
    if hyperfine -N --warmup "${BENCH_WARMUP:-2}" --min-runs "${BENCH_MIN_RUNS:-10}" \
        --export-json "$out/$name.json" "${args[@]}" > "$out/$name.txt" 2>&1; then
        echo "done: $name"
    else
        echo "FAILED: $name (see $out/$name.txt)"
    fi
}

bench startup 'true' 'exit'
bench loop 'for ((i=0;i<20000;i++)); do :; done' \
    'for ($i=0; $i -lt 20000; $i++) { }'
bench echo 'for ((i=0;i<20000;i++)); do echo hi; done >/dev/null' \
    '& { for ($i=0; $i -lt 20000; $i++) { echo hi } } > $null'
bench arithmetic 'for ((i=0;i<20000;i++)); do x=$((i*3+i%7)); done' \
    'for ($i=0; $i -lt 20000; $i++) { $x = $i*3 + $i%7 }'
bench expansion 's=abcdef; for ((i=0;i<20000;i++)); do x=${s/b/B}; y=${s:2:3}; z=${#s}; done' \
    '$s = "abcdef"; for ($i=0; $i -lt 20000; $i++) { $x = $s.Replace("b","B"); $y = $s.Substring(2,3); $z = $s.Length }'
bench function 'f() { :; }; for ((i=0;i<20000;i++)); do f; done' \
    'function f { }; for ($i=0; $i -lt 20000; $i++) { f }'
bench substitution 'for ((i=0;i<1000;i++)); do x=$(echo hi); done' \
    'for ($i=0; $i -lt 1000; $i++) { $x = $(echo hi) }'
bench program "for ((i=0;i<100;i++)); do $prog; done" \
    'for ($i=0; $i -lt 100; $i++) { C:/Windows/System32/HOSTNAME.EXE > $null }'
bench pipe 'for ((i=0;i<100;i++)); do echo hi | cat >/dev/null; done' \
    'for ($i=0; $i -lt 100; $i++) { echo hi | Out-Null }'
bench ls "cd $listed_native && for ((i=0;i<50;i++)); do ls -la >/dev/null; done" \
    "Set-Location $listed_native; for (\$i=0; \$i -lt 50; \$i++) { Get-ChildItem -Force | Out-String > \$null }"
rm -rf "$listed"
