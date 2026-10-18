---
name: true
summary: Do nothing, successfully.
kind: Bash builtins
see: false test
---

## Syntax

    true [ARG]...

    true                    succeed, doing nothing
    while true; do ...      a loop that runs until a break
    COMMAND || true         a failure that does not stop the script

## Examples

A loop that ends only at a `break`:

    n=0
    while true; do
      n=$((n + 1))
      [ $n -eq 3 ] && break
    done
    echo "$n rounds"

A failing command that must not stop a script under `set -e`:

    set -e
    grep -q dog notes.txt || true
    echo "still running"

## How it works

`true` does nothing and exits 0; it ignores its arguments. It is the condition of a
loop that runs until a `break`, and `|| true` after a command keeps its failure from
ending a script under `set -e`, or failing a pipeline under `set -o pipefail`. `:` is
the same command, with its arguments expanded.

## Options

None: every argument is ignored.

Exit status: 0.
