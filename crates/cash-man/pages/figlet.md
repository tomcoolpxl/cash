---
name: figlet
summary: Print text in big letters made of small ones.
kind: Text and file tools
see: coolfetch echo
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

    for f in standard slant small big mini smslant shadow script; do echo "$f:"; figlet -f "$f" Hi; done

Wrapped at 40 columns, broken between words:

<!-- parse only: Git Bash has no figlet to compare with -->

    figlet -w 40 "a longer message than fits"

## How it works

`figlet` draws each character of its message, or of each line of standard input, with a
FIGlet font: a file of letters made of ordinary characters. Letters are moved together
until they touch (kerning) or overlap where the font's rules allow it (smushing), and a
line that would pass the width is broken between words. It is a reimplementation of
FIGlet 2.2.5 and reads both FIGlet's fonts (`.flf`) and TOIlet's (`.tlf`, UTF-8, some in
colour), zipped or not, and FIGlet's control files (`.flc`).

cash carries 23 fonts: FIGlet's `standard`, `slant`, `small`, `big`, `banner`, `block`,
`bubble`, `digital`, `ivrit`, `lean`, `mini`, `script`, `shadow`, `smscript`,
`smshadow`, `smslant` and `term`, and TOIlet's `future`, `mono9`, `mono12`, `pagga`,
`smblock` and `smbraille`. Any other font is read from a directory: `-d DIR`, else
`$FIGLET_FONTDIR`, then `%LOCALAPPDATA%\cash\fonts`, before the built-in ones, and the
current directory last. A name with a slash or a backslash is a path.

## Options

    -f, --font=FONT       The font, by name (with or without .flf or .tlf) or path.
    -d, --directory=DIR   Look for fonts and control files in DIR first.
    -C FILE               A control file, translating characters; may be repeated.
    -N                    Forget the control files given so far.
    -D                    [\]{|}~ as German letters.
    -E                    Not as German letters (the default).

    -s                    The font's own layout (the default).
    -S                    Smush by the font's rules; overlap where it has none.
    -k                    Kerning: letters touch.
    -W                    Full width: letters side by side.
    -o                    Overlap each letter with the one before.
    -m MODE               A layout as a number: -1 full width, 0 kerning, a sum of
                          smushing rules to smush with them.

    -w, --width=COLUMNS   The output width; a line takes at most one column less.
    -t, --termwidth       The console's width.
    -l, -c, -r            Left, centred, right.
    -x                    As the font runs: right for a right-to-left font.
    -L, -R                Left to right, right to left.
    -X                    As the font runs.
    -p                    Paragraph mode: a single newline is a blank.
    -n                    Normal mode (the default): every newline ends a line.

    -I CODE               Tell: 0 the version, 1 the version number, 2 the font
                          directory, 3 the font, 4 the width, 5 the font formats.
    -v, --version         The version.
    -h, --help            The options.

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
