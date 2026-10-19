---
name: history
summary: Show or change the list of commands typed.
kind: Bash builtins
see: fc keys config vars
---

## Syntax

    history [N]
    history -c | -d OFFSET | -a | -n | -r | -w [FILE] | -s ARG...

    history          every command in the list, numbered
    history 20       the last 20
    history -c       clear the list
    history -d 5     delete entry 5
    !!               the last command again; !42 entry 42

## Examples

The last commands typed, numbered:

<!-- parse only: reads the history of an interactive shell -->
    history 5

Find a command typed before:

<!-- parse only: reads the history of an interactive shell -->
    history | grep ssh

Delete an entry, the last one, and clear the list:

<!-- parse only: changes the history of an interactive shell -->
    history -d -1
    history -c

Run the last command again, or the last one that started with `git`:

<!-- parse only: history expansion is for the prompt -->
    !!
    !git

## How it works

An interactive shell keeps each command line you type in a list, which `history`
writes, numbered. At the prompt, Up and Down walk through it and Ctrl-R searches it.
The list is read from `HISTFILE` as the shell starts and written back when it ends;
`HISTSIZE` sets how many are kept in memory and `HISTFILESIZE` how many in the file.

A script keeps no list unless it turns one on with `set -o history`. From then on each
command it reads is kept, its lines joined into one entry, and `history` in the script
lists them; what it sources or runs through `eval` is not kept. A script has no
`HISTFILE` unless it sets one, so the prompt's file gets none of it.

`!` at the prompt reuses a line from the list: `!!` the last command, `!42` entry 42,
`!-2` the one before last, `!ssh` the last that started with ssh, `!$` the last word
of the last command. The line is shown as it was expanded before it runs.

`HISTCONTROL=ignoredups:ignorespace` keeps a line equal to the last one, or one that
starts with a space, out of the list; `HISTIGNORE='ls:cd *'` keeps lines matching its
patterns out. `HISTTIMEFORMAT='%F %T '` writes when each was typed.

## Options

    N            The last N entries.
    -c           Clear the list.
    -d OFFSET    Delete entry OFFSET; a negative one counts from the end.
    -a           Append this session's new lines to the file.
    -n           Read the file's lines not read yet.
    -r           Read the file into the list.
    -w [FILE]    Write the list to the file.
    -s ARG...    Add ARGs to the list as one entry.
    -p ARG...    Write ARGs with ! expanded; keep nothing.

Exit status: 0, or 1 for an offset that is not there.

## Different in cash

- An interactive shell's file is `~/.cash_history` unless `HISTFILE` names another, so
  cash's history does not mix with Git Bash's `~/.bash_history`.
- Ctrl-R offers this directory's commands first: cash remembers the directory each
  command ran in, unless `CASH_NO_RECORDS` is set (see `help vars`).
