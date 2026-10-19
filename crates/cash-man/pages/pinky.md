---
name: pinky
summary: Show brief information about users.
kind: Coreutils written for Windows
see: who users id
---

## Syntax

    pinky [OPTION]... [USER]...
    pinky -l USER...

    pinky          the users logged on: login, name, line, idle, when
    pinky USER     the same, for USER's sessions alone
    pinky -l USER  USER's login name, real name, home directory and shell

## Examples

The users logged on, and one in long form:

<!-- parse only: Git Bash has no record of who is logged on -->

    pinky
    pinky -l "$(whoami)"

Just the logins, lines and times, with no heading:

<!-- parse only: Git Bash has no record of who is logged on -->

    pinky -qf

## How it works

`pinky` is a lighter `finger`. Without `-l` it prints a line for each session
someone is logged on to, or USER's sessions alone: the login, the account's full
name, the session's line, how long since its last key or mouse movement (blank
under a minute, `HH:MM` under a day, then days), when they logged on, and a remote
session's computer. The line is `console`, `rdp-tcp#0` for Remote Desktop, or
`disconnected`; the `?` before it stands for whether others may write to the
terminal, which Windows does not say.

With `-l`, it prints each USER's login name and full name, the profile directory
and the shell, then the file `.project` in that directory after `Project:` and
`.plan` after `Plan:`, when they are there. An account that does not exist gets
`???` for its real name.

## Options

    -l    The long format, for the USERs given.
    -b    Long format: no home directory and shell.
    -h    Long format: no project file.
    -p    Long format: no plan file.
    -s    The short format, the default.
    -f    Short format: no heading.
    -w    Short format: no full name.
    -i    Short format: no full name or remote computer.
    -q    Short format: no full name, remote computer or idle time.

Exit status: 0; 1 when `-l` has no USER.

## Different in cash

- Windows keeps no `utmp` file of who is logged on, so Git Bash's `pinky` lists no
  one. cash asks Windows for its sessions, which every edition keeps.
- The full name is the one the local account was given; the directory is the
  account's profile, `C:/Users/NAME`. An account has no login shell on Windows: the
  shell shown is `$SHELL`, or cash.
