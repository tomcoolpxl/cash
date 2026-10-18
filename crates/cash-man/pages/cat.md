---
name: cat
summary: Concatenate files to standard output.
kind: Bundled coreutils, from uutils
see: less head tail tac nl crlf
---

## Syntax

    cat [OPTION]... [FILE]...

    cat FILE                print a file
    cat FILE... > ALL       join files into one
    cat -n FILE             with line numbers
    cat -A FILE             show tabs, line ends and other unseen characters
    COMMAND | cat           pass standard input through

## Examples

Print a file:

    cat notes.txt

Number the lines, or only the lines that are not empty:

    cat -n pairs.txt
    cat -b list.txt

Squeeze runs of empty lines into one:

    cat -s list.txt

See what is in a line that looks right but is not: a CRLF file shows `^M$`:

    cat -A windows.txt

Join two files into a third:

    cat notes.txt pairs.txt > both.txt

Put standard input between two files, with `-`:

    echo between | cat notes.txt - pairs.txt

## How it works

`cat` reads each FILE in turn, or its standard input without one, and writes what it
reads to its standard output, byte for byte. `-` is standard input. Its name is from
concatenate: `cat a b > c` joins two files into a third.

The options change what is written: `-n` and `-b` number the lines, `-s` squeezes
empty lines, and `-v`, `-E`, `-T` and `-A` make characters that do not print visible:
`^I` for a tab, `$` at each line's end, `^M` for a carriage return, `M-` before a byte
above 127.

For a long file, `less` lets you scroll; to look at the start or the end, `head` and
`tail`.

## Options

    -n, --number             Number every line.
    -b, --number-nonblank    Number the lines that are not empty; beats -n.
    -s, --squeeze-blank      Write runs of empty lines as one.
    -E, --show-ends          Show $ at the end of each line.
    -T, --show-tabs          Show a tab as ^I.
    -v, --show-nonprinting   Show other control characters as ^X and bytes
                             above 127 as M-X; not a tab or a newline.
    -A, --show-all           The same as -vET.
    -e                       The same as -vE.
    -t                       The same as -vT.
    -u                       Taken and ignored, as GNU's cat does.

Exit status: 0 when every file was read; 1 when one could not be.

## Advanced

### Line endings

`cat` changes no bytes: a CRLF file stays CRLF, and joining an LF file to a CRLF one
gives a file of both. `cat -A` shows which: `^M$` ends a CRLF line, `$` an LF one.
`dos2unix` and `unix2dos` change them.

### Here-documents

`cat` with a here-document writes text from a script:

    cat > config.ini <<EOF
    url=$URL
    EOF

## Different in cash

- It is cash's own copy of uutils' `cat`, carried inside cash, so it is the same on
  every machine, with or without Git or MSYS2. Its errors are in GNU's words: `cat:
  nosuch: No such file or directory`.
