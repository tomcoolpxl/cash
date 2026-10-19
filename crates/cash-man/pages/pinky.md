---
name: pinky
summary: Show brief information about users.
kind: Coreutils written for Windows
see: who users
---

## Syntax

    pinky [USER]...

    pinky          the users logged on: login, name, line, when
    pinky USER     USER's login name and home directory

## Examples

The users logged on, and one in long form:

<!-- parse only: Git Bash has no record of who is logged on -->

    pinky
    pinky "$(whoami)"

## How it works

`pinky` is a lighter `finger`. Without a USER it prints a line for each user logged
on: the login name, the name, the session's line and when it started. With USERs, it
prints each one's login name and home directory, and the shell.

## Options

None yet: GNU's `-l`, `-s`, `-f` and the others are not there.

Exit status: 0.

## Different in cash

- Windows keeps no `utmp` file of who is logged on, so Git Bash's `pinky` lists no one.
  cash asks Windows' `quser.exe`, which Windows Pro and Enterprise have; without it,
  `pinky` names this session's account alone.
