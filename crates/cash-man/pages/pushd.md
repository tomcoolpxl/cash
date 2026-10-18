---
name: pushd
summary: Put a directory on the stack and change to it.
kind: Bash builtins
see: popd dirs cd
---

## Syntax

    pushd [-n] [DIR | +N | -N]

    pushd DIR    go to DIR, keeping the directory you were in
    pushd        swap the two directories on top: back and forth
    pushd +N     turn the stack until entry N is on top

## Examples

Go somewhere, and come back:

    mkdir -p build
    pushd build >/dev/null
    touch made-here.txt
    popd >/dev/null
    ls build

Back and forth between two directories:

    mkdir -p src docs
    cd src
    pushd ../docs >/dev/null; basename "$PWD"
    pushd >/dev/null; basename "$PWD"
    pushd >/dev/null; basename "$PWD"

## How it works

The directory stack is a list of directories with the current one on top: `dirs` shows
it, the current directory first, as entry 0. `pushd DIR` puts DIR on top and changes to
it, so the directory you were in is entry 1, and `popd` takes you back to it. `pushd`
with no directory swaps the top two, which goes back and forth between two
directories. `pushd +N` turns the stack until entry N, counting from 0 at the left of
`dirs`, is on top, and changes to it; `-N` counts from the right.

`pushd` prints the stack after, as `dirs` does: `>/dev/null` quiets it.

## Options

    -n    Put DIR on the stack under the top, without changing to it.
    +N    Turn the stack until entry N, from 0 at the left, is on top.
    -N    The same, counting from 0 at the right.

Exit status: 0, or 1 when DIR cannot be entered, or N is past the end of the stack.
