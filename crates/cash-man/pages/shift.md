---
name: shift
summary: Shift the positional parameters to the left.
kind: Bash builtins
see: set getopts
---

## Syntax

    shift [N]

    shift      drop $1: $2 becomes $1, and so on
    shift 2    drop $1 and $2

## Examples

The arguments one at a time:

    set -- red green blue
    while [ $# -gt 0 ]; do echo "$1"; shift; done

Options first, then the rest:

    set -- -v -n 3 file.txt
    while [ $# -gt 0 ]; do
      case $1 in
        -v) verbose=1; shift ;;
        -n) count=$2; shift 2 ;;
        *) break ;;
      esac
    done
    echo "verbose=$verbose count=$count rest=$*"

Too few to shift:

    set -- one
    shift 2 || echo "not enough arguments"
    echo "$1"

## How it works

`shift N` drops the first N positional parameters, `$1` to `$N`; the rest move down,
and `$#` is N less. Without N it drops one. When N is more than `$#`, nothing moves
and `shift` fails, so `shift 2 || usage` catches a missing value. In a function, it
shifts the function's own arguments.

`getopts` reads options without shifting; `shift $((OPTIND - 1))` then leaves the
operands.

## Options

    N    How many to drop: 1 without; 0 drops none.

Exit status: 0; 1 when N is more than `$#` or negative.
