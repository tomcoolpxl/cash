---
see: reset read keys
---
## Description

`stty` prints or changes the terminal's settings, in GNU coreutils' words. Bare `stty`
prints the speed and what differs from `stty sane`; `-a` prints everything in GNU's
layout; `-g` prints a string that `stty "$saved"` restores. Settings are words such as
`-echo`, `raw`, `sane`, `erase ^H` or `min 1 time 0`, several per call.

The terminal is the console standard input is attached to. With standard input a pipe or
a file, `stty` fails as GNU's does: `'standard input': Inappropriate ioctl for device`.

## Windows notes

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

## Examples

```bash
stty -echo; IFS= read -r password; stty echo; echo
saved=$(stty -g); stty raw -echo; key=$(read -r -n 1 k; printf %s "$k"); stty "$saved"
stty size          # rows and columns of the console window
stty -a            # everything, as GNU prints it
```
