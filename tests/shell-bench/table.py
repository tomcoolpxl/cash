"""One table from hyperfine's JSON exports: each benchmark's median per shell, and the cost
of one iteration once a shell's start (the `startup` benchmark) is taken off."""
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

means = {}
for folder in sys.argv[1:]:
    for name in ITERATIONS:
        path = pathlib.Path(folder) / f"{name}.json"
        if not path.exists():
            continue
        for result in json.loads(path.read_text(encoding="utf-8"))["results"]:
            wall = result["summary"]["time_wall_clock"]["median"]
            means.setdefault(name, {})[result["name"]] = wall

shells = []
for row in means.values():
    for shell in row:
        if shell not in shells:
            shells.append(shell)


def cell(name, shell):
    mean = means.get(name, {}).get(shell)
    if mean is None:
        return "-"
    if name == "startup":
        return f"{mean * 1000:.1f} ms"
    start = means.get("startup", {}).get(shell, 0.0)
    each = (mean - start) / ITERATIONS[name] * 1e6
    return f"{mean * 1000:.0f} ms ({each:.1f} us each)" if each < 1000 else f"{mean * 1000:.0f} ms ({each / 1000:.2f} ms each)"


print("| benchmark | " + " | ".join(shells) + " |")
print("|---|" + "---|" * len(shells))
for name in ITERATIONS:
    if name in means:
        print(f"| {name} | " + " | ".join(cell(name, s) for s in shells) + " |")
