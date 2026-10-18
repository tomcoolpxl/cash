#!/usr/bin/env bash
# Runs the shell benchmarks for cash, Git Bash and PowerShell 7, then for WSL's bash inside
# WSL (so wsl.exe's own start is not counted), one after the other, and prints one table:
# each workload's median, and what one of its iterations costs once the shell's start is off.
# Run from Git Bash; hyperfine must be on PATH on both sides.
#
#   bash tests/shell-bench/run.sh [CASH] [OUTDIR]
#
# CASH defaults to Scoop's cash.exe (by its real path: Scoop's shim would add a process);
# OUTDIR to a new temporary directory.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
cash=${1:-$(cygpath -m "$USERPROFILE")/scoop/apps/cash/current/cash.exe}
out=${2:-$(mktemp -d)}
mkdir -p "$out"
# Windows' own spelling of the directories: hyperfine and python are given them unconverted,
# as the commands they run must be (`MSYS_NO_PATHCONV`).
here=$(cygpath -m "$here")
out=$(cygpath -m "$out")
gitbash=$(cygpath -m "$(command -v bash)")
shells=("cash=$cash" "gitbash=$gitbash")
command -v pwsh > /dev/null && shells+=("pwsh=$(cygpath -m "$(command -v pwsh)")")
export MSYS_NO_PATHCONV=1
bash "$here/bench.sh" "$out/windows" "${shells[@]}"
if command -v wsl.exe > /dev/null; then
    wsl.exe -e bash "$(wsl.exe -e wslpath -a "$here/bench.sh" | tr -d '\r')" \
        "$(wsl.exe -e wslpath -a "$out/wsl" | tr -d '\r')" "wslbash=/usr/bin/bash"
fi
python -I "$here/table.py" "$out/windows" "$out/wsl"
echo "hyperfine's own reports: $out"
