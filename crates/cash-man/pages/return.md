---
name: return
summary: Return from a function or a sourced file.
kind: Bash builtins
see: exit source trap
---

## Syntax

    return [N]

    return      leave the function, with the last command's status
    return 0    leave it, succeeding
    return 1    leave it, failing

## Examples

A function that answers with its status:

    is_even() { return $(( $1 % 2 )); }
    if is_even 4; then echo "4 is even"; fi
    is_even 7 || echo "7 is odd"

Leaving early:

    check() {
      [ -f "$1" ] || return 1
      echo "$1 is there"
    }
    check notes.txt
    check nosuch.txt; echo "status $?"

`return` alone keeps the last command's status:

    has_dog() { grep -q dog notes.txt; return; }
    has_dog; echo "status $?"

A sourced file returns to the one that sourced it:

    printf 'echo first\nreturn 3\necho never\n' > part.sh
    . ./part.sh; echo "status $?"

## How it works

`return` ends the function it is in, and the function's status is N, or, without N,
the status of the last command run. N is taken modulo 256: `return 300` is 44. A
function that ends without `return` has the status of its last command.

In a file run with `source` or `.`, `return` ends the file, and `source` gives N.
Anywhere else it is an error; a script ends with `exit`.

A `RETURN` trap runs as the function returns (`trap`).

## Options

    N    The status, 0 to 255; a larger one is taken modulo 256.

Exit status: N, or without it the last command's.
