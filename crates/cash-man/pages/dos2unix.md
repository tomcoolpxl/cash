---
names: dos2unix unix2dos
summary: Convert CRLF line endings to LF.
summary unix2dos: Convert LF line endings to CRLF.
kind: Text and file tools
see: sed awk crlf iconv
---

## Syntax

    dos2unix [OPTION]... [FILE]...
    unix2dos [OPTION]... [FILE]...

    dos2unix FILE...        CRLF to LF, in place
    unix2dos FILE...        LF to CRLF, in place
    dos2unix -n IN OUT      a new file, IN left alone
    dos2unix -i FILE        count the line endings, changing nothing

## Examples

A CRLF file made LF, in place:

    printf 'x\r\ny\r\n' > win.txt
    dos2unix -q win.txt
    od -c win.txt

Into a new file, and the other way:

    printf 'x\r\ny\r\n' > win.txt
    dos2unix -q -n win.txt unix.txt
    unix2dos -q unix.txt
    cmp -s win.txt unix.txt && echo "back as it was"

Through a pipe:

    printf 'x\r\n' | dos2unix | od -c

## How it works

`dos2unix FILE...` converts CRLF line endings to LF, in place; `unix2dos` does the
reverse. `-n IN OUT` writes a new file instead, and with no files they convert standard
input to standard output. `-i` reports what a file holds without changing it. `-k`
keeps the file's date, `-q` says nothing of each file, and a file that looks binary is
skipped unless `-f`.

A file is replaced by writing a temporary file beside it and renaming it over the
original, so a failure never leaves it half written.

## Options

    -n, --newfile IN OUT     Write OUT, leave IN.
    -o, --oldfile FILE...    In place: the default.
    -O, --to-stdout          Write to standard output.
    -i, --info               Count the line endings; change nothing.
    -k, --keepdate           Keep the file's date.
    -q, --quiet              Say nothing of each file.
    -f, --force              Convert a file that looks binary too.
    -e, --add-eol            End the last line with a line ending.
    -r, --remove-bom         Take a byte order mark off: the default.
    -b, --keep-bom           Keep a byte order mark.
    -m, --add-bom            Put a UTF-8 byte order mark on.
    -l, --newline            Every line ending doubled.
    -7                       Each byte past ASCII a space.
    -v, --verbose            Say more of each file.

Exit status: 0, or 1 when a file cannot be read or written.

## Different in cash

- Git for Windows has the real tools only when its `usr/bin` is on `PATH`, so cash
  carries both: the one-word fix for a CRLF file works everywhere.
- Code-page, UTF-16 and Mac (CR-only) conversions are refused by name rather than done
  wrongly; `enable -n dos2unix` reaches the `PATH` copy.
