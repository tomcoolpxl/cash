---
names: test [
summary: Evaluate a conditional expression.
summary [: Evaluate a conditional expression (the same as `test`, ended by `]`).
kind: Bash builtins
see: chmod ln paths
---

## Syntax

    test EXPRESSION
    [ EXPRESSION ]

    [ -e FILE ]          FILE is there
    [ -d DIR ]           DIR is a directory
    [ -z "$S" ]          the string S is empty; -n: not empty
    [ "$A" = "$B" ]      two strings are the same
    [ "$N" -lt 10 ]      a number is less than 10

## Examples

Act when a file is there, or when it is not:

    if [ -f notes.txt ]; then echo 'notes found'; fi
    [ -e nosuch ] || echo 'no such file'

A variable empty or not, quoted so that an empty one is still a word:

    name=''
    if [ -z "$name" ]; then echo 'no name'; fi

Compare strings, and numbers:

    [ abc = abc ] && echo same
    [ 10 -ge 9 ] && echo 'at least nine'

Two conditions, both or either:

    [ -f notes.txt ] && [ -d . ] && echo both
    [ -e nosuch ] || [ -d . ] && echo either

Which file is newer:

    touch -d 2020-01-01 old.txt
    [ notes.txt -nt old.txt ] && echo 'notes.txt is newer'

## How it works

`test` works out the expression and returns 0 when it holds, 1 when it does not, and
2 when it cannot read it; it prints nothing. `[` is the same command, its last
argument a `]`, which reads well after `if` and `while`:

    if [ -d build ]; then rm -r build; fi

Each operator and each operand is a word of its own: `[ -f x ]`, never `[-f x]`.
Quote variables, `[ -n "$x" ]`: unquoted and empty, a variable is no word at all,
and the expression reads otherwise.

`[[ ... ]]` is the shell's own keyword, not this command: it does not split or glob
what it is given, and it adds `==` with patterns, `=~` with regular expressions, and
`&&` and `||` inside. In a Bash script it is usually the better choice; `test` and
`[` are POSIX's, for scripts that must run under any shell.

## Options

### Files

    -e FILE          It is there.
    -f FILE          A regular file.
    -d FILE          A directory.
    -L, -h FILE      A symbolic link or a junction.
    -s FILE          Not empty.
    -r FILE          You may read it.
    -w FILE          You may write it.
    -x FILE          You may run it: a program, or a directory to enter.
    -O FILE          You own it.
    -G FILE          Its group is yours.
    -N FILE          Changed since it was last read.
    -b, -c, -p, -S   A block device, a character device, a pipe, a socket.
    -u, -g, -k       Set-user-ID, set-group-ID, sticky: never on Windows.
    -t FD            The descriptor FD is a terminal.
    A -nt B          A is newer than B, or B is not there.
    A -ot B          A is older than B, or A is not there.
    A -ef B          A and B are the same file.

### Strings

    -z S             S is empty.
    -n S, S          S is not empty.
    A = B, A == B    The same.
    A != B           Not the same.
    A < B, A > B     Before or after in sort order; quote them: \<.

### Numbers

    A -eq B          Equal; -ne not equal.
    A -lt B          Less than; -le less or equal.
    A -gt B          Greater than; -ge greater or equal.

### Variables and options

    -v NAME          The variable NAME is set.
    -R NAME          NAME is set and is a name reference.
    -o OPTION        The shell option OPTION (set -o) is on.

### Joining

    ! EXPR           EXPR is false.
    ( EXPR )         EXPR, grouped; quote the parentheses: \( and \).
    A -a B           Both; better written [ A ] && [ B ].
    A -o B           Either; better written [ A ] || [ B ].

Exit status: 0 when the expression holds; 1 when it does not; 2 when it is wrong.

## Advanced

### How many arguments

`test` reads its arguments by their number, as POSIX says: with one, it is true when
that one is not empty (`[ x ]`, `[ "" ]` false); with two, the first is an operator
(`[ -e x ]`, `[ ! x ]`); with three, the middle one usually is. Past four, `-a` and
`-o` make it ambiguous: join separate tests with `&&` and `||`.

### Unquoted variables

With `v=''`, `[ $v = y ]` reads as `[ = y ]`: two arguments, the first taken for an
operator, and `[: =: unary operator expected`, status 2. `[ "$v" = y ]` compares an
empty string, as was meant.

## Different in cash

- `-x FILE` is true when the access list lets you run the file and it is an
  executable kind: an extension in `PATHEXT` (`.exe`, `.cmd`, ...), a `#!` line, or a
  directory. Windows grants execute on every file you own, so the access list alone
  would call `README.md` executable.
- `-O FILE` and `-G FILE` read the file's owner and group from its security
  descriptor, as `ls -l` shows them: a file you created is yours, `cmd.exe` is not.
  Git Bash calls every file yours.
- `-N FILE` compares the last write with the last access, and Windows counts a virus
  scanner's read as an access.
- `-L FILE` is true for symbolic links and junctions, not for App Execution Aliases
  or `.lnk` shortcuts.
- `-s FILE` is false for App Execution Aliases (`python.exe` from the Store), which
  are empty files that run anyway.
- Paths may be spelled `C:/x`, `/c/x` or a quoted `"C:\x"`.
