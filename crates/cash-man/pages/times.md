---
name: times
summary: Show the time used by the shell and its children.
kind: Bash builtins
see: ps ulimit
---

## Syntax

    times

    times    the processor time the shell and its commands used

## Examples

Two lines, the shell's and its commands':

    times | wc -l

After some work:

<!-- parse only: the times differ on every run -->

    sort big.txt > /dev/null
    times

## How it works

`times` prints two lines of processor time, each as user time then system time: first
the shell's own, then that of the commands it ran and waited for, such as

    0m0.031s 0m0.046s
    0m0.109s 0m0.062s

It counts time spent working, not waiting. The `time` keyword times one command.

## Options

None.

Exit status: 0.
