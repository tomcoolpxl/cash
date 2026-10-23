---
name: figlist
summary: List the fonts figlet can use, and the control files.
kind: Text and file tools
see: figlet showfigfonts
---

## Syntax

    figlist [-d DIRECTORY]

    figlist           the built-in fonts, and those in the font directory
    figlist -d DIR    and those in DIR

## Examples

The fonts in a directory of your own:

<!-- parse only: Git Bash has no figlist to compare with -->

    figlist -d ~/figlet-fonts

## How it works

`figlist` prints the default font, the font directory (`-d`, else `$FIGLET_FONTDIR`,
else `%LOCALAPPDATA%\cash\fonts`), the 23 fonts built into cash, and the fonts (`.flf`,
`.tlf`) and control files (`.flc`) in each font directory. It is FIGlet's script of the
same name, with cash's built-in fonts.

## Options

    -d, --directory=DIR   List the fonts and control files in DIR too.
    -h, --help            The options.

Exit status: 0, or 1 for a bad option.
