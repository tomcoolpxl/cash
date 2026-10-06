---
title: Installing and upgrading: Scoop, the Terminal profile, tool links, fonts
summary: What the Scoop install sets up, the optional steps, and how to upgrade while a cash window is open.
see: config keys ls which
---
## What the install does

`scoop install cash` puts `cash.exe` in place and runs three things, each quietly:

- `cash --terminal-profile`: a "cash" profile in Windows Terminal, with the logo and
  command marks on. Restart Terminal if it does not show. `cash --remove-terminal-profile`
  takes it out; the uninstall does that too.
- `cash --link-tools`: when you have asked for the tool links before (below), they are
  refreshed to point at the new `cash.exe`.
- `cash --init-rc --once`: the first time only, and only when you have neither `~/.bashrc`
  nor `~/.cashrc`, a starter `~/.bashrc` with a prompt, aliases and history settings.
  `cash --init-rc` writes it again by hand.

`cash doctor` checks the result: that `sh`, `bash` and `cash` reach cash, which Windows
programs a builtin hides, whether the tool links are current, and what is worth adding.

## Optional, once

- **Tool links on PATH.** `cash --link-tools --add-to-path` makes `ls.exe`, `sort.exe`,
  `sed.exe` and every other bundled tool a program on `PATH`, so editors, `make`, `npm`
  and scripts outside cash can run them. `cash --unlink-tools` removes them. `help which`
  says how a builtin answers to a path.
- **Icons in `ls --icons`.** They need a Nerd Font in the Terminal profile:
  `scoop install nerd-fonts/CascadiaMono-NF`, then `cash --terminal-profile` again, which
  picks the font up.
- **Completions with descriptions** for git, winget, docker and 700 more commands:
  `scoop install extras/carapace-bin`. Nothing else to configure.

## Upgrading

`scoop update cash`. Scoop refuses to replace a program that is running; with a cash
window open, either close it or let Scoop go ahead, once:

```
scoop config ignore_running_processes true
```

The open windows keep running the old `cash.exe` until they close; new tabs get the new
one. Scoop keeps the old versions on disk; `scoop cleanup cash` removes them.
