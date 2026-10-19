---
name: stty
summary: Print or change the terminal's settings, in GNU's words, on the console's modes.
kind: Terminal and clipboard
see: reset read keys
---

## Syntax

    stty [-a | -g]
    stty SETTING...

    stty -a          every setting, as GNU prints it
    stty size        the console window's rows and columns
    stty -echo       keys typed are not shown
    stty "$saved"    put back what stty -g saved

## Examples

A password read without showing it, and one key read without Enter:

<!-- parse only: changes the reader's console -->

    stty -echo; IFS= read -r password; stty echo; echo
    saved=$(stty -g); stty raw -echo
    key=$(read -r -n 1 k; printf %s "$k"); stty "$saved"
    stty size

## How it works

`stty` prints or changes the terminal's settings, in GNU coreutils' words. Bare `stty`
prints the speed and what differs from `stty sane`; `-a` prints everything in GNU's
layout; `-g` prints a string that `stty "$saved"` restores. Settings are words such as
`-echo`, `raw`, `sane`, `erase ^H` or `min 1 time 0`, several per call.

The terminal is the console standard input is attached to. With standard input a pipe or
a file, `stty` fails as GNU's does: `'standard input': Inappropriate ioctl for device`.

## Options

    -a, --all       Every setting, in GNU's layout.
    -g, --save      A string that stty reads back to restore them.
    size            The rows and columns of the window.
    echo, -echo     Show the keys typed, or not.
    icanon, -icanon A line at a time, or each key.
    isig, -isig     Ctrl-C a signal, or a character.
    raw, cooked     Every key as it comes; back to lines.
    sane            Everything as a fresh console has it.
    min N, time N   For -icanon: how many keys, how long to wait.

Exit status: 0; 1 for a setting it does not know, or when standard input is not a
terminal.

## Different in cash

Four settings are the console's own and take effect at once: `echo` (whether keys are
shown), `icanon` (whether input is collected a line at a time), `isig` (whether Ctrl-C is
a signal rather than a character) and `opost` (output processing). The other settings and
the special characters (`intr`, `erase`, `eof`, speeds, parity, delays, ...) have no
counterpart on a console. They are accepted so that scripts written for a terminal run,
remembered for this shell, and printed back by `-a` and `-g`, so a script that saves and
restores them sees what it set.

`rows N` and `cols N` are accepted and ignored: `size` reports the console window, and a
script that tells the kernel a size does not expect the window to change. `-F DEVICE` is
refused; there is only the console.

`sane` and `cooked` also put the console back as a fresh one is, as `reset` does. cash
does that itself before each prompt, so a setting lasts for the current command line or
script, as a program's would.

`read` at a console shows the keys itself, and leaves them unshown while echo is off, so
the common `stty -echo; read -r password; stty echo` hides the password as in Bash.
