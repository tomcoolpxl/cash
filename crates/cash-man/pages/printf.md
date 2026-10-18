---
name: printf
summary: Write arguments formatted by a format string.
kind: Bash builtins
see: echo read date
---

## Syntax

    printf [-v VAR] FORMAT [ARGUMENT]...

    printf '%s\n' TEXT...       each argument on a line of its own
    printf '%5d\n' NUMBER       a number in five columns
    printf -v VAR FORMAT ...    into the variable VAR, not the output

## Examples

Each argument on a line of its own: the format is used again while arguments are left:

    printf '%s\n' one two three
    printf '%s=%s\n' a 1 b 2

Columns: a name 10 wide to the left, a number 5 wide to the right:

    printf '%-10s|%5d|\n' name 42

Numbers in other bases, and with decimals:

    printf '%x %o %#x\n' 255 8 255
    printf '%.2f %05.1f\n' 3.14159 2.5

Into a variable rather than the output:

    printf -v line '%s-%s' x y
    echo "$line"

A value quoted so that the shell reads it back as it was:

    printf '%q\n' 'a b' "it's"

The character code of a letter, with a quote before it:

    printf '%d\n' "'A"

## How it works

`printf` writes FORMAT with each `%` directive replaced by the next ARGUMENT, and the
backslash escapes in FORMAT read (`\n`, `\t`, `\\`). It writes no newline of its own:
put `\n` in the format. When arguments are left after the format is used, the format
is used again; when they run out, a directive writes nothing (or 0 for a number).

Unlike `echo`, `printf` treats every argument the same, whatever it holds, so
`printf '%s\n' "$x"` is the safe way to write a value that may start with `-` or hold
backslashes.

### Directives

    %s        a string              %b        a string, its escapes read
    %d, %i    a whole number        %u        a number, unsigned
    %x, %X    hexadecimal           %o        octal
    %f        a decimal number      %e, %E    in exponent form
    %g, %G    %f or %e, shorter     %c        the first character
    %q        quoted for the shell  %(FMT)T   a time, strftime's FMT
    %%        a %

Between `%` and the letter: flags (`-` left, `0` zeros, `+` a sign, a space, `#`
the base's mark), a width, and `.` with a precision. `*` takes the width or the
precision from an argument: `printf '%*d' 6 42`.

## Options

    -v VAR    Put the output in the variable VAR, not on standard output.
    --        The end of the options; FORMAT may then start with -.

A number argument may be decimal, `0x` hexadecimal, `0` octal, or a quote and a
character for its code.

Exit status: 0 on success; 1 when an argument is not a valid number or the output
cannot be written, the rest still written; 2 for a bad option.

## Advanced

### Times

`%(FMT)T` formats a time, the seconds since 1970 given as the argument: `printf
'%(%Y-%m-%d)T\n' -1` writes today's date (`-1` is now, `-2` the shell's start).

### Reusing the format

`printf '%s,' a b c` writes `a,b,c,`; `printf '%s\n' "${array[@]}"` writes an array a
line each.
