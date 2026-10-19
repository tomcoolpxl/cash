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

Who is logged on:

<!-- parse only: Git Bash has no record of who is logged on -->

    users
    users | wc -w

## How it works

`users` prints the names of the users logged on, sorted, a space between, each once.

## Options

None.

Exit status: 0.

## Different in cash

- Windows keeps no `utmp` file of who is logged on, so Git Bash's `users` prints
  nothing; cash's names the account of this session.
