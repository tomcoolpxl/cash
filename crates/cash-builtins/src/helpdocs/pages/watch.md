---
see: top keys
---
## Description

`watch` runs a command every 2 seconds (`-n SECONDS`, a tenth of a second at least) and
shows its output full screen, with procps-ng's options: the first line says
`Every 2.0s: command` and the machine's name and time, the second how long the run took
and its status; `-t` leaves both out. `-d` highlights what changed since the last run,
`-d=permanent` what has changed since the first. `-g` ends when the output changes and
`-q N` when it has stayed the same for N runs, both with status 0. `-e` freezes on a
failing command and ends with its status once a key is pressed; `-b` beeps on one. `-c`
keeps the command's colours, `-w` cuts long lines at the edge instead of wrapping them,
`-f` scrolls the output on instead of redrawing it, `-p` counts the interval from the
start of each run, `-r` does not rerun the command when the window is resized, and
`-x` runs the words as one command rather than as shell source.

`q` and Ctrl-C leave, with status 0. Space runs the command at once. `s` saves the
screen as text into `watch_DATE-TIME` in the working folder, or the folder `-s` names.

## Windows notes

Windows has no `watch`, and Git for Windows does not carry one either. The command runs
in a copy of this shell, as procps runs it in a child with `sh -c`, so functions and
aliases are visible, and `cd` or a variable it sets does not reach the shell. Standard
output and standard error are shown together. The output must go to a terminal: in a
pipe or a file `watch` fails with status 1 instead of drawing into it.

The screen is drawn as `top` draws it, each frame over the last on the alternate
screen, and put back on leaving. Without `-c`, whole escape sequences are dropped from
the output, where procps keeps all but the escape character.

## Examples

```bash
watch -n 1 'ls | wc -l'            # how many files, every second
watch -d ls -l                     # highlight what changes
watch -n 0.5 -g 'cat status.txt'   # return as soon as the file changes
watch -e 'curl -fsS localhost/health'   # freeze when the check fails
```
