---
name: figlet
summary: Print text in big letters made of small ones.
kind: Text and file tools
see: figlist showfigfonts coolfetch
---

## Syntax

    figlet [OPTION]... [MESSAGE]

    figlet Hello               Hello in big letters
    figlet -f slant Hello      in another font
    figlet -c -f small Hello   centred in the window
    echo Hello | figlet        standard input, a line at a time

## Examples

A banner at the start of a script:

<!-- parse only: Git Bash has no figlet to compare with -->

    figlet -f slant "Build done"

Centred in the console, in a smaller font:

<!-- parse only: Git Bash has no figlet to compare with -->

    figlet -c -f small "$(hostname)"

Every built-in font, to choose one:

<!-- parse only: Git Bash has no figlet to compare with -->

    for f in standard slant small big mini smslant shadow script; do
        echo "$f:"; figlet -f "$f" Hi
    done

Wrapped at 40 columns, broken between words:

<!-- parse only: Git Bash has no figlet to compare with -->

    figlet -w 40 "a longer message than fits"

In TOIlet's colours, boxed, and saved with its colours as code page 437 art:

<!-- parse only: Git Bash has no figlet to compare with -->

    figlet -f future --metal -F crop:border Done
    figlet -f mono9 --rainbow -E ansi Hello > hello.ans

A logo in a true-colour gradient, framed, and lolcat's rainbow:

<!-- parse only: Git Bash has no figlet to compare with -->

    figlet -f slant --gradient=sunset --border=rounded --padding=2 cash
    figlet -f big --gradient=#00c6ff,#7f00ff --direction=diagonal Hello
    figlet --lolcat "Hello, World!"

In pixels: the built-in 8x8 font in quadrants, and an installed Windows font:

<!-- parse only: Git Bash has no figlet to compare with -->

    figlet --pixel=quadrant Hello
    figlet --winfont="Segoe UI" --size=24 --gradient=ocean Hello

## How it works

`figlet` draws each character of its message, or of each line of standard input, with a
FIGlet font: a file of letters made of ordinary characters. Letters are moved together
until they touch (kerning) or overlap where the font's rules allow it (smushing), and a
line that would pass the width is broken between words. It is a reimplementation of
FIGlet 2.2.5 with TOIlet 0.3's additions: it reads both FIGlet's fonts (`.flf`) and
TOIlet's (`.tlf`, UTF-8, some in colour), zipped or not, and FIGlet's control files
(`.flc`), and it has TOIlet's filters and output formats.

Filters (`-F`) change each line as it is printed: `crop` takes off the blanks around
it, `rainbow` and `metal` colour it, `flip` and `flop` mirror it, `180`, `left` and
`right` turn it, and `border` boxes it. Several are given with colons (`-F crop:border`)
or with more `-F`s, and run in order. Colour reaches a console, unless `NO_COLOR` is
set; into a pipe or a file it is left out, unless `-E` names a format or `--color=always`
asks for it.

cash adds more: gradients in true colour, from a named palette (`--gradient=list`) or
colours of your own; lolcat's rainbow; a shadow; the outline of solid letters; frames in
five styles with padding; a background colour; space between letters and between lines.
They are filters too, and run in the order given with the others. Windows Terminal and
the Windows console show true colour; `--colors=256` or `--colors=16` is for a terminal
that shows fewer.

Pixel fonts draw letters as pixels with block characters, several pixels to a cell:
`--pixel` uses the built-in 8x8 font, `--winfont` any font installed in Windows, drawn
at `--size` pixels. `half` blocks show in every font; `quadrant` in any with block
elements; `sextant` and `octant` need Windows Terminal or Cascadia Code; `braille` most
fonts have. `--dec` uses no font at all: Windows Terminal draws the text itself twice
as large.

TheDraw fonts (`.tdf`), the colour fonts of DOS's ANSI art, are read too: put them in a
font directory and name them with `-f` like any other. Colour and block fonts are read,
not outline fonts; a line is as high as the letters on it, as in TheDraw.

cash carries 23 fonts: FIGlet's `standard`, `slant`, `small`, `big`, `banner`, `block`,
`bubble`, `digital`, `ivrit`, `lean`, `mini`, `script`, `shadow`, `smscript`,
`smshadow`, `smslant` and `term`, and TOIlet's `future`, `mono9`, `mono12`, `pagga`,
`smblock` and `smbraille`. Any other font is read from a directory: `-d DIR`, else
`$FIGLET_FONTDIR`, then `%LOCALAPPDATA%\cash\fonts`, before the built-in ones, and the
current directory last. A name with a slash or a backslash is a path.

## Options

    -f, --font=FONT     The font, by name or path; .flf, .tlf or .tdf may be
                        left off.
    -d, --directory=DIR Look for fonts and control files in DIR first.
    -C FILE             A control file, translating characters; may be
                        given more than once.
    -N                  Forget the control files given so far.
    -D                  [\]{|}~ as German letters.

    -s                  The font's own layout (the default).
    -S                  Smush by the font's rules, or overlap.
    -k                  Kerning: letters touch.
    -W                  Full width: letters side by side.
    -o                  Overlap each letter with the one before.
    -m MODE             A layout as a number: -1 full width, 0 kerning,
                        a sum of smushing rules to smush with them.

    -w, --width=COLUMNS The output width; a line takes at most one
                        column less.
    -t, --termwidth     The console's width.
    -l, -c, -r          Left, centred, right.
    -x                  As the font runs: right for a right-to-left font.
    -L, -R              Left to right, right to left.
    -X                  As the font runs.
    -p                  Paragraph mode: a single newline is a blank.
    -n                  Every newline ends a line (the default).

    -F, --filter=LIST   Filters, separated by colons; -F list says what
                        each does.
    --rainbow, --gay    The rainbow filter.
    --metal             The metal filter.
    -E, --export=FORMAT utf8 (the default: UTF-8, with colour), ansi
                        (code page 437, colour, CRLF line ends) or text
                        (no colour ever); -E list names them.
    --color[=WHEN]      Colour: auto (a console, NO_COLOR unset),
                        always or never.
    --colors=DEPTH      true (the default), 256 or 16 colours.

    --gradient=SPEC     A gradient: a palette's name (--gradient=list)
                        or colours with commas: names, #rgb, #rrggbb.
    --direction=DIR     horizontal (the default), vertical, diagonal.
    --lolcat            lolcat's rainbow; --seed=N for the same colours
                        each time, --spread=F and --freq=F as lolcat's.
    --shadow            A shade down and right of the text.
    --outline           Only the edges of solid letters.
    --border[=STYLE]    A frame: single (the default), double, rounded,
                        heavy or ascii.
    --padding=N         N blank columns inside the frame, half as many
                        rows.
    --background=COLOR  Every cell on COLOR.
    --letter-spacing=N  N blank columns after every letter.
    --line-gap=N        N blank rows after every line of text.

    --pick              Choose a font from a list, MESSAGE drawn in the
                        one under the cursor; its name is printed:
                        figlet -f "$(figlet --pick Hi)" Hi

    --pixel[=MODE]      The built-in pixel font, 8x8 (also -f 8x8),
                        drawn in half (the default), full, quadrant,
                        sextant, octant or braille characters.
    --winfont=NAME      An installed Windows font, drawn as pixels.
    --size=PIXELS       Its size (16); --bold, --italic.
    --dec[=big|wide]    The text itself in the terminal's double-size
                        lines: twice as high and wide, or as wide.

    -I CODE             Tell: 0 the version, 1 the version number, 2 the
                        font directory, 3 the font, 4 the width, 5 the
                        font formats.
    -v, --version       The version.
    -h, --help          The options.

Exit status: 0, or 1 for a bad option or a font or control file that cannot be read.

## Different in cash

### Difference from FIGlet

The message is Unicode text: `-C utf8` is never needed, and works even where `utf8.flc`
is not. The control files' commands for other input encodings (DBCS, HZ, Shift-JIS,
ISO 2022) are read and not applied.

With neither `-w` nor `-t`, the width is the console's when the output goes to one;
into a pipe or a file it is FIGlet's 80, so a script gets what FIGlet would give it.

A CRLF line end in the input is one newline, so a Windows text file prints no blank
figure between its lines.

`-E` is TOIlet's output format; FIGlet's own `-E` only turned `-D` off again, which is
the default anyway.

### Difference from TOIlet

There is one command, `figlet`; TOIlet's options are part of it. Layout is FIGlet's,
which TOIlet follows only roughly. The formats are `utf8`, `ansi` and `text`; TOIlet's
HTML, IRC, SVG and others are not carried. TOIlet colours a pipe; cash does so only when
`-E` names a format or `--color=always` asks.
