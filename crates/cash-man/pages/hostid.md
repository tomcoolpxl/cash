---
name: hostid
summary: Print the numeric identifier of this host.
kind: Coreutils written for Windows
see: hostname uname
---

## Syntax

    hostid

    hostid    eight hex digits that name this machine

## Examples

Eight hex digits:

    hostid | grep -cE '^[0-9a-f]{8}$'

## How it works

`hostid` prints a number that names the machine, as eight hex digits. Scripts use it
as a short, stable key for the machine: the same on every run.

## Options

None.

Exit status: 0.

## Different in cash

- Windows has no host id of the kind Unix keeps: cash's is a hash of the computer's
  name, so it changes when the machine is renamed. Git Bash's is another hash, so the
  two differ.
