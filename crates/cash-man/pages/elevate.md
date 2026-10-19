---
name: elevate
summary: Run a command elevated, in a new window, through UAC.
kind: Windows commands
see: sudo su detach elevation
---

## Syntax

    elevate [-q] COMMAND [ARG]...

    elevate regedit        regedit elevated, in its own window
    elevate -q services.msc    without the note about the job object

## Examples

Programs that want a window of their own, elevated:

<!-- parse only: asks UAC on the reader's machine -->

    elevate -q regedit
    elevate cmd

## How it works

`elevate COMMAND` asks UAC to start a command elevated, in a new window, and returns
without waiting for it, for a program that should have a window of its own. For output
in this terminal, and the command's status, use `sudo`.

## Options

    -q, --quiet    Leave out the note that the program outlives cash.

Exit status: 0 when the command started; 1 when it could not be, or UAC's prompt was
declined.

## Different in cash

- The command starts in the shell's directory and is found as the shell would run it (a
  script runs through cash), but from your own environment: UAC passes on none of the
  shell's variables.
- It is not waited for, so `elevate`'s status says only that the command started, not
  how it ended.
- An elevated program cannot be put in cash's job object, so it outlives cash and cash
  cannot end it. `elevate` says so on standard error; `-q` keeps quiet.
