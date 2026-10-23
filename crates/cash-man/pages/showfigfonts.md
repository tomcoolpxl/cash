---
name: showfigfonts
summary: Show every font figlet can use, a word drawn in each.
kind: Text and file tools
see: figlet figlist
---

## Syntax

    showfigfonts [-d DIRECTORY] [WORD]...

    showfigfonts          each font's name, drawn in it
    showfigfonts Hello    Hello in every font

## Examples

Every font, to choose one, a screen at a time:

<!-- parse only: Git Bash has no showfigfonts to compare with -->

    showfigfonts Hello | less

## How it works

`showfigfonts` prints, for every font `figlet` can find (the 23 built into cash and
those in the font directories), its name and a colon, then WORD drawn in it, or the
font's own name without one, then a blank line. It is FIGlet's script of the same name.
`figlet --pick` does the same in a list, one font at a time.

## Options

    -d, --directory=DIR   Show the fonts in DIR too.
    -h, --help            The options.

Exit status: 0, or 1 for a bad option.
