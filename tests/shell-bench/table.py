"""One table from the shell benchmarks run.sh keeps in results/: each workload's median per
shell, and the cost of one iteration once a shell's start (the `startup` workload) is taken
off. With no arguments, the latest run of each system (windows, linux); with arguments, the
run directories given. Each run's machine follows the table, from its machine.txt."""
import json
import pathlib
import sys

ITERATIONS = {
    "startup": 1,
    "loop": 20000,
    "echo": 20000,
    "arithmetic": 20000,
    "expansion": 20000,
    "function": 20000,
    "substitution": 1000,
    "program": 100,
    "pipe": 100,
    "ls": 50,
}

RESULTS = pathlib.Path(__file__).resolve().parent / "results"


def run_order(run):
    """A run directory's place in time: DATE-SYSTEM, or DATE-HHMM-SYSTEM for a later run
    of the same day."""
    parts = run.name.split("-")
    return (parts[:3], parts[3] if len(parts) == 5 else "0000")


def latest_runs():
    latest = {}
    if RESULTS.is_dir():
        for run in sorted((p for p in RESULTS.iterdir() if p.is_dir()), key=run_order):
            latest[run.name.rsplit("-", 1)[-1]] = run
    # Windows' four shells first, then Linux's bash.
    return sorted(latest.values(), key=lambda run: run.name.endswith("-linux"))


def machine(run):
    """A run's machine.txt as a dictionary: `cash: cash 2.1.1 (git:...)` is `cash`."""
    path = run / "machine.txt"
    if not path.exists():
        return {}
    pairs = (line.split(":", 1) for line in path.read_text(encoding="utf-8").splitlines())
    return {key.strip(): value.strip() for key, value in pairs if value}


def label(shell, run):
    """A column's heading, saying which shell it is: cash with its version, as a table that
    only said `cash` did not tell 2.1.1's run from 2.0.0's."""
    if shell == "cash":
        words = machine(run).get("cash", "").split()
        return f"cash {words[1]}" if len(words) > 1 else "cash"
    if shell == "pwsh":
        words = machine(run).get("pwsh", "").split()
        return f"PowerShell {words[1].split('.')[0]}" if len(words) > 1 else "PowerShell"
    return {"gitbash": "Git Bash", "wslbash": "WSL bash", "linuxbash": "Linux native"}.get(
        shell, shell
    )


def median(result):
    """The median wall-clock time of one command, from hyperfine 2's export or 1's."""
    if "summary" in result:
        return result["summary"]["time_wall_clock"]["median"]
    return result["median"]


runs = [pathlib.Path(arg) for arg in sys.argv[1:]] or latest_runs()
if not runs:
    sys.exit(f"no results in {RESULTS}: run run.sh first")

found = []  # (run, workload, shell, seconds)
for run in runs:
    for path in sorted(run.rglob("*.json")):
        if path.stem in ITERATIONS:
            for result in json.loads(path.read_text(encoding="utf-8"))["results"]:
                shell = result.get("name") or result["command"]
                found.append((run, path.stem, shell, median(result)))

# A shell is its own column; one that two runs give is told apart by the run.
owners = {}
for run, _, shell, _ in found:
    owners.setdefault(shell, set()).add(run)
medians = {}
columns = []
for run, workload, shell, seconds in found:
    name = label(shell, run)
    column = name if len(owners[shell]) == 1 else f"{name} ({run.name})"
    medians.setdefault(workload, {})[column] = seconds
    if column not in columns:
        columns.append(column)


def cell(workload, column):
    seconds = medians.get(workload, {}).get(column)
    if seconds is None:
        return "-"
    if workload == "startup":
        return f"{seconds * 1000:.1f} ms"
    start = medians.get("startup", {}).get(column, 0.0)
    each = (seconds - start) / ITERATIONS[workload] * 1e6
    if each < 1000:
        return f"{seconds * 1000:.0f} ms ({each:.1f} us each)"
    return f"{seconds * 1000:.0f} ms ({each / 1000:.2f} ms each)"


print("| benchmark | " + " | ".join(columns) + " |")
print("|---|" + "---|" * len(columns))
for workload in ITERATIONS:
    if workload in medians:
        print(f"| {workload} | " + " | ".join(cell(workload, c) for c in columns) + " |")

for run in runs:
    machine = run / "machine.txt"
    print(f"\n{run.name}:")
    if machine.exists():
        for line in machine.read_text(encoding="utf-8").splitlines():
            print(f"- {line}")
