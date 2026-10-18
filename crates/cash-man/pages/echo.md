---
name: echo
summary: Write the arguments to standard output.
kind: Bash builtins
see: printf cat read
---

## Syntax

    echo [-neE] [ARG]...

    echo TEXT            write TEXT and a newline
    echo -n TEXT         without the newline
    echo -e 'A\tB'       with backslash escapes read: \t a tab, \n a newline
    echo "$VAR"          a variable's value, as it is

## Examples

Write a line:

    echo hello world

Without the newline at the end, to be followed by more:

    echo -n 'no newline, '; echo then this

With escapes: a tab, a newline, and `\c` to stop there:

    echo -e 'a\tb\nc'
    echo -e 'one\c two'; echo

A variable quoted keeps its spaces; unquoted, the shell splits it first:

    v='  spaced  out '
    echo "$v"
    echo $v

Write to a file, or add to one:

    echo 'first line' > out.txt
    echo 'next line' >> out.txt

## How it works

`echo` writes its arguments separated by single spaces, and a newline after them. The
shell has already expanded them: variables, globs, and the splitting of unquoted
words, so quote what should stay as it is: `echo "$file"`, `echo '*'`.

`-n` leaves out the newline. `-e` reads backslash escapes in the arguments; `-E`, the
default, does not. Options may be joined, `-ne`, and only these are options: any other
word starting with `-`, `--` too, is written as it is.

### Escapes with -e

    \a     alert (bell)            \b     backspace
    \c     nothing more: stop      \e     escape
    \f     form feed               \n     new line
    \r     carriage return         \t     tab
    \v     vertical tab            \\     a backslash
    \0NNN  the byte of octal NNN   \xHH   the byte of hex HH
    \uHHHH, \UHHHHHHHH             the Unicode character, in UTF-8

## Options

    -n    Leave out the newline at the end.
    -e    Read backslash escapes in the arguments.
    -E    Do not read them (the default).

Exit status: 0, unless the output cannot be written.

## Advanced

### echo or printf

`echo`'s options differ between shells and systems, and a value that starts with `-`
can be taken for one: `echo "$x"` with `x=-n` writes nothing. For text that may hold
anything, `printf '%s\n' "$x"` is the safe way.

### xpg_echo

`shopt -s xpg_echo` makes `echo` read escapes without `-e`, as POSIX's echo does.

## Different in cash

- `\u` and `\U` write the character in UTF-8, whatever the locale; Git Bash in the C
  locale writes something else for a character outside ASCII.
- `echo` is a builtin, as in Bash; Git's and MSYS2's `/usr/bin/echo` are not run.
  `type -a echo` lists them.
