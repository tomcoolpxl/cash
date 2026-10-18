---
name: printenv
summary: Print environment variables.
kind: Bundled coreutils, from uutils
see: env export vars
---

## Syntax

    printenv [OPTION]... [NAME]...

    printenv            every variable programs get
    printenv NAME       one's value
    printenv A B        several, a line each

## Examples

One variable's value:

    export GREETING=hello
    printenv GREETING

Whether a variable is exported: `printenv` sees only what programs get:

    LOCAL=x
    printenv LOCAL || echo 'not exported'

Several at once:

    export A=1 B=2
    printenv A B

## How it works

`printenv` writes the environment, the exported variables a program gets: every one
as `NAME=value`, or for each NAME given, its value alone. A NAME that is not in the
environment writes nothing and makes the status 1.

A shell variable not exported is not in the environment, so `printenv` tells
exported from not, where `echo "$V"` does not.

## Options

    -0, --null    End each with a NUL byte, not a newline.

Exit status: 0 when every NAME was there; 1 otherwise.

## Different in cash

- It is uutils' `printenv`, carried inside cash, so it is the same on every machine,
  with or without Git or MSYS2.
- `PATH` is in its Windows form, with semicolons, as programs get it; `$PATH` in the
  shell is `/c/...:/c/...` (see `help paths`).
