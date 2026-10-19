#!/usr/bin/env bash
# Runs the shell benchmarks on this machine and keeps the results in the repository, then
# prints the table of every machine's latest results (table.py).
#
# On Windows, from Git Bash: cash, Git Bash and PowerShell 7, then WSL's bash inside WSL
# (so wsl.exe's own start is not counted), one after the other. On Linux: its bash. Run
# both on the same machine, rebooted and idle, and the table compares all five shells.
# hyperfine must be on PATH on each side (`scoop install hyperfine`, `apt install
# hyperfine`).
#
#   bash tests/shell-bench/run.sh [CASH] [OUTDIR]
#
# CASH, on Windows, defaults to Scoop's cash.exe (by its real path: Scoop's shim would add
# a process). OUTDIR defaults to tests/shell-bench/results/DATE-windows or DATE-linux,
# which gets hyperfine's reports, one directory per side, and machine.txt: the CPU, the
# memory, the system and each shell's version.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
case $(uname -s) in
    Linux) os=linux ;;
    MINGW* | MSYS* | CYGWIN*) os=windows ;;
    *) echo "run.sh: runs on Windows (Git Bash) or Linux, not $(uname -s)" >&2; exit 2 ;;
esac
cash=${1:-}
out=${2:-}
if [ -z "$out" ]; then
    out=$here/results/$(date +%Y-%m-%d)-$os
    [ -e "$out" ] && out=$here/results/$(date +%Y-%m-%d-%H%M)-$os
fi
mkdir -p "$out"
# The table shows every system's latest run kept, or this run alone when it is kept
# elsewhere.
table=()
case $(cd "$out" && pwd) in
    "$here"/results/*) ;;
    *) table=("$out") ;;
esac

# The first line of what a command prints, or "-" when it fails.
first() { "$@" 2>/dev/null | tr -d '\r' | sed -n '1{s/^[[:space:]]*//;s/[[:space:]]*$//;p;}' | grep . || echo -; }

if [ "$os" = linux ]; then
    {
        echo "system: $(. /etc/os-release && echo "$PRETTY_NAME"), Linux $(uname -r)"
        echo "cpu: $(sed -n 's/^model name[[:space:]]*: //p' /proc/cpuinfo | head -1)"
        echo "threads: $(nproc)"
        echo "memory: $(awk '/^MemTotal/ { printf "%.0f GB", $2 / 1048576 }' /proc/meminfo)"
        echo "hyperfine: $(first hyperfine --version)"
        echo "linuxbash: $(first bash --version)"
    } > "$out/machine.txt"
    bash "$here/bench.sh" "$out/linux" "linuxbash=$(command -v bash)"
    python3 -I "$here/table.py" "${table[@]}"
    echo "kept in: $out"
    exit
fi

cash=${cash:-$(cygpath -m "$USERPROFILE")/scoop/apps/cash/current/cash.exe}
# Windows' own spelling of the directories: hyperfine and python are given them unconverted,
# as the commands they run must be (`MSYS_NO_PATHCONV`).
here=$(cygpath -m "$here")
out=$(cygpath -m "$out")
[ ${#table[@]} -eq 0 ] || table=("$out")
gitbash=$(cygpath -m "$(command -v bash)")
shells=("cash=$cash" "gitbash=$gitbash")
command -v pwsh > /dev/null && shells+=("pwsh=$(cygpath -m "$(command -v pwsh)")")
export MSYS_NO_PATHCONV=1

wmi() { first powershell.exe -NoProfile -NonInteractive -Command "$1"; }
{
    echo "system: $(wmi '$o = Get-CimInstance Win32_OperatingSystem; "$($o.Caption) $($o.Version)"')"
    echo "cpu: $(wmi '(Get-CimInstance Win32_Processor | Select-Object -First 1).Name')"
    echo "threads: $(wmi '(Get-CimInstance Win32_ComputerSystem).NumberOfLogicalProcessors')"
    echo "memory: $(wmi '"{0:N0} GB" -f ((Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory / 1GB)')"
    echo "hyperfine: $(first hyperfine --version)"
    echo "cash: $(CASH_NO_OFFER=1 first "$cash" --version)"
    echo "gitbash: $(first bash --version)"
    command -v pwsh > /dev/null &&
        echo "pwsh: PowerShell $(first pwsh -NoProfile -Command '$PSVersionTable.PSVersion.ToString()')"
    if command -v wsl.exe > /dev/null; then
        echo "wslbash: $(first wsl.exe -e bash --version)"
        echo "wsl: $(first wsl.exe -e sh -c '. /etc/os-release && echo "$PRETTY_NAME, Linux $(uname -r)"')"
    fi
} > "$out/machine.txt"

bash "$here/bench.sh" "$out/windows" "${shells[@]}"
if command -v wsl.exe > /dev/null; then
    wsl.exe -e bash "$(wsl.exe -e wslpath -a "$here/bench.sh" | tr -d '\r')" \
        "$(wsl.exe -e wslpath -a "$out/wsl" | tr -d '\r')" "wslbash=/usr/bin/bash"
fi
python -I "$here/table.py" "${table[@]}"
echo "kept in: $out"
