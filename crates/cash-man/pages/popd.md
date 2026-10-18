---
name: popd
summary: Take a directory off the stack and change to the new top.
kind: Bash builtins
see: pushd dirs cd
---

## Syntax

    popd [-n] [+N | -N]

    popd       back to the directory before the last pushd
    popd +N    drop entry N from the stack
    popd -n    drop the entry under the top, staying where you are

## Examples

Back where you were:

    mkdir -p logs
    pushd logs >/dev/null
    touch today.log
    popd >/dev/null
    ls logs

Into two directories, and out again:

    mkdir -p a/b
    pushd a >/dev/null; pushd b >/dev/null
    basename "$PWD"
    popd >/dev/null; basename "$PWD"
    popd >/dev/null; ls -d a

Nothing to go back to:

    popd 2>/dev/null || echo "the stack is empty"

## How it works

`popd` takes the top off the directory stack, the current directory, and changes to
the new top: the directory you were in before the last `pushd`. `popd +N` takes off
entry N, counting from 0 at the left of `dirs`, and `-N` counting from the right;
taking off any but the top leaves you where you are. `-n` takes off the entry under the
top, and changes nothing.

`popd` prints the stack after, as `dirs` does: `>/dev/null` quiets it.

## Options

    -n    Take off the entry under the top; do not change directory.
    +N    Take off entry N, counting from 0 at the left.
    -N    Take off entry N, counting from 0 at the right.

Exit status: 0, or 1 when the stack is empty or N is past its end.
