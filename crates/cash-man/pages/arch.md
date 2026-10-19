---
name: arch
summary: Print the machine's architecture.
kind: Bundled coreutils, from uutils
see: uname
---

## Syntax

    arch

    arch    the processor's kind: x86_64, as uname -m prints it

## Examples

The same as `uname -m`:

    arch
    [ "$(arch)" = "$(uname -m)" ] && echo "the same"

## How it works

`arch` prints the machine's architecture, `x86_64` on an Intel or AMD 64-bit machine:
the same as `uname -m`.

## Options

None.

Exit status: 0.
