---
name: getopt
summary: Parse command options for a script, util-linux style.
kind: Text and file tools
see: getopts shift
---

## Syntax

    getopt -o SHORT [-l LONG] [-n NAME] -- ARG...

    getopt -o ab: -- "$@"              -a, and -b with a value
    getopt -o v -l verbose -- "$@"     -v and --verbose
    args=$(getopt ...); eval set -- "$args"    read back for a loop

## Examples

A command line put in order: options first, then `--`, then the operands:

    getopt -o ab: -- -a -b x y
    getopt -o v -l verbose,output: -- --verbose --output=out.txt file

The usual loop:

    set -- -v --output out.txt file
    args=$(getopt -o vo: -l verbose,output: -- "$@") || exit 2
    eval set -- "$args"
    while true; do
      case $1 in
        -v|--verbose) verbose=1; shift ;;
        -o|--output) out=$2; shift 2 ;;
        --) shift; break ;;
      esac
    done
    echo "verbose=$verbose out=$out rest=$*"

## How it works

`getopt` is util-linux's: it normalises a script's command line, long options
included, for a `while`/`case` loop to read. Clusters (`-ab`), `--name=value`, unique
prefixes of long options, `-a` for long options after one dash, and `POSIXLY_CORRECT`
work as there. Its output is single-quoted; read it back with `eval set --`. An option
it does not know is reported, as `-n NAME` names the script, and the status is 1.

`getopts`, Bash's builtin, takes short options only, but needs no `eval`.

## Options

    -o, --options=SHORT     The short options; a : after one takes a value.
    -l, --longoptions=LONG  The long options, a comma between.
    -n, --name=NAME         The name the errors give.
    -a, --alternative       Long options after one dash too.
    -q, --quiet             No error messages.
    -Q, --quiet-output      No output; the status says.
    -s, --shell=SHELL       Quote for sh or bash: the default.
    -u, --unquoted          No quotes: words with spaces break.
    -T, --test              Exit 4: this is util-linux's getopt.

Exit status: 0; 1 for an option it does not know or a missing value; 2 for a bad call;
4 for `-T`.

## Different in cash

It hides Git's MSYS `getopt.exe`, whose output is the same but whose arguments pass
through Cygwin's command-line splitting. `-s csh` and `-s tcsh` are refused: cash is no
C shell.
