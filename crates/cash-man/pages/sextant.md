---
name: sextant
summary: Play a shooter drawn in the terminal with text.
kind: cash's shell commands
see: coolfetch figlet
---

## Syntax

    sextant [OPTION]...

    sextant              play
    sextant --demo       watch it play by itself
    sextant --glyphs     check how the terminal draws the cells

## Examples

Check that the terminal shows the cell patterns:

<!-- parse only: Git Bash has no sextant to compare with -->

    sextant --glyphs

A quiet game at 30 frames a second, watched:

<!-- parse only: Git Bash has no sextant to compare with -->

    sextant --demo --fps 30

## How it works

The keys:

    Left Right, A D      move
    Space, Up            fire; hold it down to keep firing
    Enter                play again after the game is over
    q, Esc, Ctrl-C       quit

Keys are held from press to release, so moving and firing at the same time works.

`sextant` draws pixels, two wide and three high to a terminal cell, in 24-bit colour,
with one of the 64 patterns of a 2x3 cell in each cell. A wave of enemies sways at the
top and sends some of its members down in dives that shoot; clear the wave for the next,
faster one. Three lives; the score and the wave are printed when you quit.

The patterns come from a soft font that `sextant` sends the terminal (DEC's DECDLD,
character bitmaps), so no font needs anything; Windows Terminal draws them. Fonts that
have Unicode's sextants (Symbols for Legacy Computing) can draw them instead, with
`--unicode`. `sextant --glyphs` prints every pattern both ways: a question mark or a box
in the first place means the font has no sextants, a plain character in the second means
the terminal has no soft fonts.

It needs a terminal: with its output elsewhere, it says so and plays nothing. The colours
are the picture, so they are drawn whatever `NO_COLOR` says.

## Options

    --demo        The game plays by itself, again after each game over.
    --unicode     Draw with the font's sextants, not the soft font.
    --fps N       Frames a second, 1 to 240; 60 by default.
    --glyphs      Print the 64 cell patterns both ways, and play nothing.

Exit status: 0; 1 when the output is not a terminal; 2 for a wrong option.
