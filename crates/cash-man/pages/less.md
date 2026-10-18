---
names: less more
summary: Page through text a screen at a time.
summary more: Page through text, quitting at its end.
kind: Text and file tools
see: cat head tail man
---

## Syntax

    less [OPTION]... [FILE]...
    more [FILE]...

    less FILE           page through FILE
    COMMAND | less      page through a command's output
    less -N FILE        with line numbers
    less -S FILE        long lines cut, not wrapped

## Examples

Page through a file or a command's output, `q` to quit:

<!-- parse only: waits for keys -->
    less app.log
    git log | less

With line numbers, and long lines cut at the screen's edge:

<!-- parse only: waits for keys -->
    less -N -S app.log

When the output is not a terminal, `less` writes the text as `cat` would:

    less notes.txt | head -1

## How it works

`less` shows a file, or its standard input, a screen at a time, and waits for keys:

    Space, f    a page forward        b           a page back
    Enter, j    a line forward        k           a line back
    g           the start             G           the end
    /TEXT       search forward        ?TEXT       search back
    n, N        the next match, back  q           quit

`more` is the same pager with `more`'s habits: it quits at the end and shows
`--More--(45%)`.

When standard output is not a terminal, both are `cat`, as GNU less is, so
`cmd | less > out` writes the text and nothing else.

## Options

    -N, --LINE-NUMBERS    Number the lines.
    -S, --chop-long-lines Cut long lines rather than wrap them.
    -F, --quit-if-one-screen  Quit at once when the text fits on one screen.
    -e, --quit-at-eof     Quit at the end, as more does.
    -R, -X                Taken, for scripts that pass them: colours always
                          show, and the screen is never cleared.

Exit status: 0, or 1 when a FILE cannot be read.

## Different in cash

- It is cash's own pager, carried inside cash: no `less.exe` is needed.
