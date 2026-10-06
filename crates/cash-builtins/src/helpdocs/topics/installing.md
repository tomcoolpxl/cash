---
title: Installing and upgrading: the installer, Scoop, the Terminal profile, tool links, fonts
summary: What the installer and the Scoop install set up, the optional steps, and how to upgrade while a cash window is open.
see: config keys ls which
---
## The installer

`cash-vX.Y.Z-setup.exe`, on the releases page (https://github.com/tomcoolpxl/cash/releases),
installs cash for your account alone: no administrator, no UAC prompt. It puts the files
in `%LOCALAPPDATA%\Programs\cash\X.Y.Z\`, points the `current` folder beside them at that
version, puts `current` on your PATH so `cash` is a command everywhere, adds the Windows
Terminal profile and writes the starter `~/.bashrc` the first time. Two options:

- **Make the tools programs on PATH**, on by default: `ls.exe`, `sed.exe`, `awk.exe` and
  the rest in `%LOCALAPPDATA%\Programs\cash\bin`, first on your user PATH, so editors,
  `make`, `npm`, PowerShell and scripts outside cash get cash's tools ahead of Git's.
- **Also install Scoop**, off by default: runs Scoop's own installer last, for the rest of
  your command-line tools (`scoop install jq`).

For scripts and winget, the setup runs silently:

```
cash-v1.5.0-setup.exe /VERYSILENT /SUPPRESSMSGBOXES /NORESTART /TASKS=links,scoop
```

`/DIR=` names another folder; `/TASKS=` lists the options to turn on (`links`, `scoop`),
none when empty. Uninstall from Settings > Apps > Installed apps: the tool links, the
Terminal profile, the PATH entries and the files go; your `~/.bashrc`, history and
config stay.

The setup is not signed. Downloaded with a browser, Windows shows SmartScreen's
"unknown publisher" prompt once; "More info", then "Run anyway". A download by winget or
Scoop gets no such prompt.

### Upgrading an installed cash

```
cash --update --check    # says whether a newer release is out
cash --update            # fetches it, checks its SHA-256 and installs it
```

The new version goes into a folder of its own and `current` moves to it: the windows
you have open keep running the version they started with, new windows get the new one.
Version folders no window runs any more are removed. A cash installed by Scoop says
`scoop update cash` instead; a cash unpacked from the zip by hand says where to download
the new one.

## Portable: the zip

The release zip holds `cash.exe` and the licence notices, nothing else is needed: the C
runtime is linked in, every tool and this help are inside the one file. Unpack it
anywhere and run it. At its first interactive prompt a portable cash asks, once, whether
to put itself on your PATH (so `cash` is a command in any window), whether to add the
Windows Terminal profile and whether to put its tools on PATH for other programs; Enter
means no, and the answers are kept in `%LOCALAPPDATA%\cash\portable-offer`
(delete that file to be asked again; `CASH_NO_OFFER=1` never asks). Nothing changes
without a yes, and the same things stay commands at any time: `cash --add-to-path`,
`cash --terminal-profile` and `cash --link-tools --add-to-path`, with
`--remove-from-path`, `--remove-terminal-profile` and `--unlink-tools` to undo them.
`cash doctor` says whether new windows will find cash. `cash --update --check` says when a newer zip is on the
releases page; a portable cash does not replace itself.

## What the Scoop install does

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

With that set, Scoop still prints a warning and a table of the cash processes it found
before it goes on: the window you typed `scoop update` in is one of them. The open windows
keep running the old `cash.exe` until they close; new tabs get the new one. Scoop keeps
the old versions on disk; `scoop cleanup cash` removes them.
