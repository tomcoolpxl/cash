---
name: nohup
summary: Run a command immune to hangups, its output to `nohup.out`.
kind: Processes and network
see: detach disown job-control
draft: yes
---

## How it works

`nohup COMMAND` runs a command with GNU `nohup`'s streams: input from a terminal is
replaced by an empty one, output to a terminal goes to `nohup.out` in the current directory
(or the home directory), and errors to a terminal follow the output.

## Different in cash

On Windows a hangup is the console closing, and what cash started ends with it, so
`nohup cmd &` does not keep a program running after you close the window. Start such a
program with `detach` instead.
