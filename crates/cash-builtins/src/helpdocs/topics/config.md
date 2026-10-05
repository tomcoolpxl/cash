---
title: Configuration: config.toml, options and startup files
summary: The config file's keys and defaults, cash's command-line options, and the startup files it reads.
see: cashctl shopt set keys vars
---
## The config file

`%APPDATA%\cash\config.toml`. A missing file means the defaults; a file that does not
parse is reported and the defaults are used. Unknown keys are ignored. `cash --config
FILE` reads another file (and fails if it is missing); `cash --no-config` reads none. An
option on the command line wins over the file, the file over the default.

```
[ui]
syntax-highlighting = true            # colour the line as you type

[experimental]
terminal-shell-integration = true     # OSC 133 command marks, OSC 9;9 folder
zsh-hooks = false                     # zsh-style preexec and precmd functions
```

- `syntax-highlighting`: default `true`; `--enable-highlighting=false` turns it off for
  one run.
- `terminal-shell-integration`: default `true`. Windows Terminal and VS Code use the marks
  to jump between commands and to open new tabs in the same folder.
  `--enable-terminal-integration=false` turns it off for one run.
- `zsh-hooks`: default `false`; `--enable-zsh-hooks` turns it on.

## Startup files

- An interactive shell reads `%ProgramData%\cash\cashrc` for every user, then
  `~/.bashrc`, then `~/.cashrc`, so cash-only lines can go in the last without breaking
  Git Bash. `--norc` reads none of them; `--rcfile FILE` reads FILE instead.
- A login shell (`cash -l`) reads `%ProgramData%\cash\profile`, then the first of
  `~/.bash_profile`, `~/.bash_login` and `~/.profile`; `--noprofile` skips them.
- A script reads `$BASH_ENV` if it is set, as Bash does.
- `cash --init-rc` writes a starter `~/.bashrc` when you have neither file.

## Command-line options

Bash's: `-c COMMAND`, `-s`, `-i`, `-l`/`--login`, `-e`, `-u`, `-x`, `-v`, `-n`, `-f`,
`-C`, `-o OPTION`, `+o OPTION`, `-O SHOPT`, `+O SHOPT`, `--posix`, `--norc`,
`--noprofile`, `--rcfile FILE`, `--noediting`. `cash --help` lists them all.

cash's own:

- `cash doctor`: check the tools on `PATH` and what cash will run.
- `cash help [NAME]`: this help, from PowerShell or cmd.
- `cash --terminal-profile`: add a cash profile to Windows Terminal;
  `--remove-terminal-profile` takes it away.
- `cash --link-tools [--add-to-path] [DIR]`: hard links (`ls.exe`, `sed.exe`, ...) that
  programs outside cash can run; `--unlink-tools` removes them.
- `--disable-color`, `--disable-bracketed-paste`, `--noenv`, `--xtrace-file FILE`.

`cash doctor` and `cash help` give way to a script of that name in the current folder.

## Changing the running shell

- `shopt` and `set -o`: Bash's options, plus cash's `winpaths` (see `help paths`).
  Globbing ignores case unless `shopt -u nocaseglob`.
- `cashctl gui-apps close`: GUI programs close when cash exits, for this session;
  `cashctl gui-apps outlive` is the default (see `help job-control`).
