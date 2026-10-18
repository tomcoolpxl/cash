---
name: tr
summary: Translate or delete characters.
kind: Bundled coreutils, from uutils
see: sed cut crlf
---

## Syntax

    tr [OPTION]... SET1 [SET2]

    tr a-z A-Z            lower case to upper case
    tr -d '\r'            delete carriage returns: CRLF to LF
    tr -s ' '             squeeze runs of spaces into one
    tr ',' '\n'           commas to newlines
    tr -cd '[:alnum:]'    delete all but letters and digits

## Examples

Lower case to upper case:

    echo hello | tr a-z A-Z

Turn a CRLF file into an LF one:

    tr -d '\r' < windows.txt | od -c | head -2

Squeeze runs of a character into one:

    echo 'a    b   c' | tr -s ' '

One item per line:

    echo 'a,b,c' | tr ',' '\n'

Keep only digits, deleting the rest:

    echo 'phone: 555-0123' | tr -cd '0-9'; echo

Classes of characters:

    echo 'Hello World' | tr '[:upper:]' '[:lower:]'

## How it works

`tr` reads only its standard input and writes it with characters changed: each
character of SET1 becomes the character at the same place in SET2. With `-d` the
characters of SET1 are deleted; with `-s` a run of the same character from the last
set is squeezed into one; `-c` takes every character that is not in SET1.

A set is characters, ranges (`a-z`, `0-9`), escapes (`\n`, `\t`, `\r`, `\\`) and
classes: `[:alpha:]`, `[:digit:]`, `[:alnum:]`, `[:upper:]`, `[:lower:]`,
`[:space:]`, `[:punct:]`. When SET2 is shorter than SET1, its last character is
repeated.

`tr` takes no file names: give it a file with `<`.

## Options

    -d, --delete            Delete the characters of SET1.
    -s, --squeeze-repeats   Squeeze runs of a character of the last set.
    -c, -C, --complement    Every character not in SET1.
    -t, --truncate-set1     Cut SET1 to the length of SET2 first.

Exit status: 0 on success; 1 when a set is wrong.

## Different in cash

- It is uutils' `tr`, carried inside cash, so it is the same on every machine, with or
  without Git or MSYS2.
