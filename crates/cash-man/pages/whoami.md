---
name: whoami
summary: Print the current user's name.
kind: Bundled coreutils, from uutils
see: id logname
---

## Syntax

    whoami

    whoami    the name of the account the shell runs as

## Examples

The same name Windows keeps in `USERNAME`:

    [ "$(whoami)" = "$USERNAME" ] && echo "whoami is USERNAME"

A per-user directory:

<!-- parse only: names the reader's own account -->

    mkdir -p "/c/builds/$(whoami)"

## How it works

`whoami` prints the name of the account the shell runs as, without its domain: what
`id -un` prints, and what Windows keeps in `%USERNAME%`.

## Options

None.

Exit status: 0, or 1 when the account cannot be read.

## Different in cash

- Windows' own `whoami.exe` prints `DOMAIN\user`, and has `/groups` and `/priv`; inside
  cash, `whoami` is this one, and `whoami.exe /groups` reaches Windows' own.
