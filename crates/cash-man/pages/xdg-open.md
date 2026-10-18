---
name: xdg-open
summary: Open a file, directory or URL with its default program (`start`, under the name scripts try first).
kind: Windows commands
see: start
draft: yes
---

## Examples

    xdg-open report.pdf
    xdg-open .  # this directory in Explorer
    xdg-open https://github.com/tomcoolpxl/cash
    # how scripts probe
    if command -v xdg-open >/dev/null; then xdg-open "$url"; fi

## How it works

`xdg-open TARGET` opens a file, a directory or a URL with the program Windows has for it, as
`start` does: it is `start` under the name a cross-platform script tries first, before
`open` and `start`. It takes one argument, as xdg-open 1.2.1 does, and `--help`,
`--manual` and `--version`. It does not wait for the program.

### Exit codes

- 0: the program was started.
- 1: a mistake on the command line: no argument, two arguments, or an option that is
  not `--help`, `--manual` or `--version`.
- 2: the file does not exist.
- 4: Windows could not open it: no program is registered for it, or that program
  refused.

## Different in cash

Anything with a scheme of two or more letters (`https:`, `mailto:`, `ms-settings:`) goes
to Windows as written; everything else is a path, resolved against the shell's directory,
so a drive letter is not a scheme. A path that does not exist is refused before Windows
is asked. No command processor sees the argument, so `&` and `%` in a URL mean nothing.
