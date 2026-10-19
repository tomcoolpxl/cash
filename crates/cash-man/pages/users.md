---
name: users
summary: Print the names of the users logged on.
kind: Coreutils written for Windows
see: who pinky whoami
---

## Syntax

    users

    users    the names of the users logged on, on one line

## Examples

Who is logged on, and how many sessions:

<!-- parse only: Git Bash has no record of who is logged on -->

    users
    users | wc -w

## How it works

`users` prints the account of each session someone is logged on to, sorted, a space
between. An account logged on to two sessions, at the machine and over Remote
Desktop, is named twice, as GNU's `users` names an account once for each login.

## Options

None.

Exit status: 0; 1 for a FILE.

## Different in cash

- Windows keeps no `utmp` file of who is logged on, so Git Bash's `users` prints
  nothing. cash asks Windows for its sessions, which every edition keeps. A FILE to
  read the records from is refused, as there is none.
