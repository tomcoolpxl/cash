---
name: yes
summary: Print a line over and over until killed.
kind: Bundled coreutils, from uutils
see: head seq
---

## Syntax

    yes [STRING]...

    yes            y, line after line
    yes no         no, line after line
    yes | CMD      answer y to every question CMD asks

## Examples

As many lines as are taken:

    yes | head -3
    yes no | head -2

A thousand lines of test input:

    yes line | head -1000 | wc -l

Answering an installer that asks:

<!-- parse only: names a program of the reader's own -->

    yes | ./install.sh

## How it works

`yes` writes its arguments, a space between them, or `y` without any, on a line, over
and over, as fast as it can, until what reads it stops: `yes | head -3` ends when
`head` has its three lines. It answers a command that asks y or n, and makes test
input.

## Options

None: the arguments are the line.

Exit status: it runs until its output is closed.
