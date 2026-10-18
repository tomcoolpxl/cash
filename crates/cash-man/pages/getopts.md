---
name: getopts
summary: Read a script's options, one at a time.
kind: Bash builtins
see: getopt set
---

## Syntax

    getopts OPTSTRING NAME [ARG]...

    while getopts 'ab:' opt; do ... done    -a, and -b with a value
    $OPTARG                                 the value of -b
    shift $((OPTIND - 1))                   leave the operands in $@

## Examples

A script's options, `-v` and `-o` with a value, then its operands:

    set -- -v -o out.txt one two
    while getopts 'vo:' opt; do
      case $opt in
        v) echo verbose ;;
        o) echo "output: $OPTARG" ;;
      esac
    done
    shift $((OPTIND - 1))
    echo "operands: $*"

Options joined, `-vx`, as GNU tools take them:

    set -- -vx file
    while getopts 'vx' opt; do echo "option $opt"; done

An option that is not one, reported in the script's own words, with `:` first:

    set -- -z
    while getopts ':v' opt; do
      [ "$opt" = '?' ] && echo "unknown option: -$OPTARG"
    done

## How it works

Each call reads the next option from the positional parameters (or from the ARGs
given) and puts its letter in NAME; it returns non-zero when there are none left, so
`while getopts ...` loops over them. OPTSTRING lists the letters taken; a letter with
`:` after it takes a value, which goes in `OPTARG`. `OPTIND` is the index of the next
argument to read, so `shift $((OPTIND - 1))` leaves the operands.

An option not in OPTSTRING puts `?` in NAME and writes an error; with `:` at the start
of OPTSTRING, the error is the script's to write: NAME is `?` and `OPTARG` the letter,
or NAME is `:` for a missing value. Options end at the first word that is not one, or
at `--`. Long options (`--verbose`) are not read: `getopt` reads those.

Exit status: 0 while there is an option; 1 at their end.
