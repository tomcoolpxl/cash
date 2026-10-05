---
see: getopts
---
## Description

`getopt` is util-linux's: it normalises a script's command line, long options
included, for a `while`/`case` loop to read. Clusters (`-ab`), `--name=value`, unique
prefixes of long options, `-a` for long options after one dash, and `POSIXLY_CORRECT`
work as there. Its output is single-quoted; read it back with `eval set --`.

`getopts`, Bash's builtin, takes short options only, but needs no `eval`.

## Windows notes

It hides Git's MSYS `getopt.exe`, whose output is the same but whose arguments pass
through Cygwin's command-line splitting. `-s csh` and `-s tcsh` are refused: cash is no
C shell.

## Example

```
args=$(getopt -o vo: -l verbose,output: -- "$@") || exit 2
eval set -- "$args"
while true; do
  case $1 in
    -v|--verbose) verbose=1; shift ;;
    -o|--output) out=$2; shift 2 ;;
    --) shift; break ;;
  esac
done
```
