---
name: time
summary: Time a command.
kind: Bash builtins
see: times timeout
---

## Syntax

    time [-p] PIPELINE
    time [-p] [-v] [-f FORMAT] [-o FILE [-a]] [-q] COMMAND [ARG]...

    time make              how long make took: real, user and sys
    time -p make           the same in the POSIX format
    command time -v make   every measure, one a line
    command time -f %e make
                           the seconds alone

## Examples

How long a command takes:

<!-- parse only: the times differ from run to run -->

    time sleep 1
    time -p sleep 1

The time as a number a script can use, through the command:

<!-- parse only: the times differ from run to run -->

    command time -f %e -o took.txt sleep 1
    cat took.txt

## How it works

`time` written as the first word of a command is a keyword of the shell: it times the
whole pipeline after it, and writes on standard error how long it took (`real`, the
wall clock) and how much processor time it used (`user`, in the program itself, and
`sys`, in Windows for it). `-p` writes them as POSIX has it, `real 1.00`, and
`TIMEFORMAT` sets the format otherwise.

A keyword is one only when it is written as such. `time` reached any other way, through
a variable (`t=time; $t cmd`), `command time`, `xargs time` or `/usr/bin/time`, is the
`time` command, with the options of GNU's `time` program: `-p` for the POSIX format, `-f`
for a format of your own, `-v` for every measure, `-o FILE` to write the report to FILE
(`-a` to add to it), and `-q` to leave out the line that says the command ended with a
status other than 0. Its status is the command's, and 127 when the command cannot be
found.

In a `-f` FORMAT, `%e` is the wall clock in seconds, `%E` the same as `m:ss.cc`, `%U`
and `%S` the user and system seconds, `%P` the share of the processor, `%x` the status,
`%C` the command, `%Z` the page size and `%%` a percent sign; `\n` and `\t` are a new
line and a tab.

## Options

    -p, --portability    The POSIX format: real, user and sys seconds.
    -f, --format=FORMAT  The command: the report in FORMAT.
    -o, --output=FILE    The command: the report to FILE, not to stderr.
    -a, --append         The command, with -o: add to FILE.
    -v, --verbose        The command: every measure, one a line.
    -q, --quiet          The command: no line for a status other than 0.

Exit status: the command's; 127 when it cannot be found.

## Different in cash

- Git Bash has the keyword alone: there `t=time; $t cmd` and `command time cmd` say
  `time: command not found`, as Windows has no `time` program; cash's `time` command
  answers them, as `/usr/bin/time` does on Linux.
- Windows gives no count of a command's memory, page faults, swaps or context switches
  as it ends: those measures are 0, as GNU's `time` gives 0 for what a system does not
  measure.
