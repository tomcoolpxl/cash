---
name: who
summary: Show who is logged on.
kind: Coreutils written for Windows
see: users pinky whoami
---

## Syntax

    who [OPTION]... [am i]

    who          each user logged on: name, line and when
    who -H       with a heading
    who am i     this session's line alone
    who -q       the names, and how many

## Examples

Who is on, and how many:

<!-- parse only: Git Bash has no record of who is logged on -->

    who -H
    who am i
    who -q

## How it works

`who` prints a line for each user logged on to the machine: the name, the session's
line (`console`, or a remote session's), and when it started. `-H` adds a heading,
`-m` or `who am i` keeps to this shell's own account, and `-q` prints only the names,
then `# users=N`.

## Options

    -H, --heading    A heading line.
    -m               Only this shell's own account, as who am i.
    -q, --count      The names, and how many there are.

Exit status: 0.

## Different in cash

- Windows keeps no `utmp` file of who is logged on, so Git Bash's `who` prints nothing.
  cash asks Windows' `quser.exe`, which Windows Pro and Enterprise have, and shows its
  times as Windows writes them; without it, `who` names this session's account alone.
- GNU's other options (`-a`, `-b`, `-u`, ...) are not there yet.
