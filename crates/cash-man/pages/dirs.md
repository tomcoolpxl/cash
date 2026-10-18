---
name: dirs
summary: Show the directory stack.
kind: Bash builtins
see: pushd popd cd
---

## Syntax

    dirs [-clpv] [+N | -N]

    dirs       the stack on one line, the current directory first
    dirs -v    one a line, numbered
    dirs +1    entry 1 alone: where popd goes
    dirs -c    empty the stack

## Examples

How deep, what is under the top, and an empty stack again:

    mkdir -p one two
    pushd one >/dev/null
    pushd ../two >/dev/null
    dirs -p | wc -l
    basename "$(dirs +1)"
    dirs -c; dirs -p | wc -l

The stack, numbered:

<!-- parse only: names directories of the reader's own -->

    cd ~/src
    pushd ~/docs
    dirs -v

## How it works

`dirs` prints the directory stack that `pushd` and `popd` keep: the current directory
first, as entry 0, then the ones under it, on one line with a space between, and your
home directory as `~`. With `-v` each is on a line of its own after its number, so
after the last example's `pushd`:

     0  ~/docs
     1  ~/src

`-p` prints one a line without the numbers, `-l` the full paths with no `~`, and `+N`
only entry N, counting from 0 at the left; `-N` counts from the right. `-c` empties the
stack but for the current directory.

## Options

    -c    Empty the stack.
    -l    Full paths: the home directory not written as ~.
    -p    One directory a line.
    -v    One a line, after its number.
    +N    Only entry N, counting from 0 at the left.
    -N    Only entry N, counting from 0 at the right.

Exit status: 0, or 1 when N is past the end of the stack.
