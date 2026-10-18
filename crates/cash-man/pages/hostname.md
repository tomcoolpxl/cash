---
name: hostname
summary: Show the machine's name.
kind: Coreutils written for Windows
see: uname coolfetch vars
draft: yes
---

## How it works

`hostname` prints the machine's name; `-s` the short name, `-f` the fully qualified
one, `-d` the DNS domain. It cannot change the name.

## Different in cash

Windows has two spellings of one name: the DNS API answers `desktop-tomc`, while
`%COMPUTERNAME%` and Windows' tools answer `DESKTOP-TOMC`. cash's `hostname`, `uname -n`
and `$HOSTNAME` all give the second, so `[ "$(hostname)" = "$COMPUTERNAME" ]` holds.
Windows' own `hostname.exe` cannot change the name either; inside cash, the name is this
builtin.
