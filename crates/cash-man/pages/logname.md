---
name: logname
summary: Print the user's login name.
kind: Coreutils written for Windows
see: whoami id
---

## Syntax

    logname

    logname    the name of the account that logged in

## Examples

The same as `whoami` in a shell nobody switched user in:

    [ "$(logname)" = "$(whoami)" ] && echo "the same"

## How it works

`logname` prints the login name of the account that logged in, without its domain.
`whoami` prints the account the shell runs as now, which differs after `su`.

## Options

None.

Exit status: 0, or 1 when there is no login name.

## Different in cash

- The login name is Windows' `USERNAME`.
