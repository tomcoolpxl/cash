---
name: who
summary: Show who is logged on.
kind: Coreutils written for Windows
see: users pinky whoami uptime
---

## Syntax

    who [OPTION]... [am i]

    who          each user logged on: name, line and when
    who -H       with a heading
    who am i     this session alone
    who -q       the names, and how many
    who -b       when the machine started
    who -u       with each session's idle time and process
    who -a       all of it

## Examples

Who is on, and how many:

<!-- parse only: Git Bash has no record of who is logged on -->

    who -H
    who am i
    who -q

When the machine started, and how long each session has been idle:

<!-- parse only: Git Bash has no record of who is logged on -->

    who -b
    who -uH

In a script, the number of users logged on:

<!-- parse only: Git Bash has no record of who is logged on -->

    n=$(who -q | sed -n 's/^# users=//p')

## How it works

`who` prints a line for each session someone is logged on to: the account, the
session's line, and when they logged on. The line is the session's station,
`console` at the machine itself or `rdp-tcp#0` for Remote Desktop, and
`disconnected` for a session left running with no one connected to it. A remote
session's computer follows in parentheses.

With `-u` two columns come after the time: how long since the last key or mouse
movement in that session (`.` for under a minute, `HH:MM` under a day, `old` past
it, `?` where Windows does not say), and the process id of the session's
`explorer.exe`, the Windows shell, which stands for a Unix login's shell. `-T` puts
a column before the line for whether others may write to the terminal, which is `?`
here.

## Options

    -a, --all        -b, -u and -T together, with an EXIT column.
    -b, --boot       When the machine started.
    -H, --heading    A heading line.
    -m               Only this shell's session, as who am i.
    -q, --count      The names, and how many there are.
    -s, --short      Name, line and time only, the default.
    -T, -w, --mesg   Whether others may write to the terminal.
    -u, --users      Idle time and process id too.
    -d, -l, -p, -r, -t
                     Dead processes, login processes, init's processes, the
                     run level, the last clock change: Windows has none.
    --lookup         Accepted; a remote session's computer is shown by name.

Exit status: 0; 1 for a FILE or a third word.

## Different in cash

- Windows keeps no `utmp` file of who is logged on, so Git Bash's `who` prints
  nothing. cash asks Windows for its sessions, which every edition keeps. A FILE
  to read the records from is refused, as there is none.
- The time is the logon, in the machine's time zone, as `2026-10-19 08:12`.
