---
title: Variables cash reads and sets
summary: The `CASH_*` variables, the Bash variables cash sets, and how Windows' variables are seen.
see: export paths crlf keys config
spec: D3 D5 D24 D31 D49 D61 D70
---
## Names ignore case

Windows variable names ignore case, and so do cash's (D31): `$Path`, `$path` and `$PATH`
are one variable, and an exact match wins over a near one. The usual names (`PATH`,
`HOME`, `TEMP`, `USERNAME`, `SYSTEMROOT`, `APPDATA`, ...) are upper-cased as cash starts.
The cost: `$Foo` and `$FOO` cannot be two variables.

## cash's own

- `CASH_VERSION`: cash's version. Test it to tell cash from Git Bash in a shared
  `.bashrc`: `if [ -n "$CASH_VERSION" ]; then ...`.
- `CASH_EOL`: exported as `lf`, the bundled `sed` and `awk` treat CR as data, as on
  Linux; unset, CRLF files stay CRLF (D49, `help crlf`).
- `CASH_TRANSIENT_PS1`: when set, a finished command's prompt is redrawn as this; it
  takes what `PS1` takes (D61).
- `CASH_PS_ALT`: a prompt on the right side of the line.
- `CASH_MAX_SUBSHELLS`: how many background jobs and subshells may run at once, read at
  startup; default 256 (D70).
- `CASH_DEBUG_SESSION`: when set, cash says at startup whether its session job object
  and UTF-8 console are in place.
- `PID`: cash's process id, read-only; the same as `BASHPID`.

## Bash's, as cash sets them

- `PATH`: reads Unix-style, `/c/Windows:/c/tools`; programs get the Windows form (D5,
  `help paths`).
- `HOME`: from `USERPROFILE` when not set, spelled `C:/Users/me` (D3).
- `TMPDIR`: from `TEMP` when not set. `/tmp` is that folder.
- `SHELL` and `BASH`: cash's own path. `BASH_VERSION` is `5.3.15(1)-release`, the Bash
  cash follows.
- `OSTYPE` is `windows`; `MACHTYPE` is like `x86_64-pc-windows`; `HOSTNAME` is the
  machine's name.
- `HISTFILE`: `~/.cash_history` unless set, so cash's history does not mix with Git Bash's.
- `PS1`: `\s-\v\$ ` in an interactive shell unless set; `PS2` `> `; `PS4` `+ `.
- `SHLVL`, `PWD`, `OLDPWD`, `PPID`, `UID`, `EUID`, `RANDOM`, `SECONDS`, `EPOCHSECONDS`,
  `PIPESTATUS`, `FUNCNAME`, `BASH_SOURCE`, `LINENO` and the rest work as in Bash.
- `COLUMNS` and `LINES` follow the terminal before each prompt.

## Bash's, as cash reads them

`PS0`, `PS1`, `PS2`, `PS3`, `PS4`, `PROMPT_COMMAND` (a string or an array), `CDPATH`
(for `cd`; entries may be spelled `C:/x` or `/c/x`, separated by `:` or `;`),
`HISTSIZE`, `HISTFILESIZE`, `HISTCONTROL`, `HISTIGNORE`, `HISTTIMEFORMAT`, `TIMEFORMAT`,
`TMOUT`, `FUNCNEST`, `GLOBSORT`, `BASH_ENV` (for scripts), `ENV` (for `sh`), `FCEDIT`
and `EDITOR` (for `fc`), `VISUAL` and `EDITOR` (for `Ctrl-X Ctrl-E`), `TZ` (exported),
`LS_COLORS` (for `ls`), `NO_COLOR` (no colour in errors and `top`), `IFS`, `OPTIND`,
`OPTARG`, `REPLY`.

Not read: `MSYSTEM`, `INPUTRC`, `MAIL`, `FIGNORE`, `EXECIGNORE`,
`PROMPT_DIRTRIM`, `BASH_COMPAT`.

## Windows', as cash reads them

- `PATHEXT`: the extensions a command name may leave out (`.exe`, `.cmd`, ...).
- `COMSPEC`: what runs `.bat` and `.cmd` files; `cmd.exe` if unset.
- `USERPROFILE`, `HOMEDRIVE` and `HOMEPATH`: where `HOME` comes from.
- `TEMP` and `TMP`: `TMPDIR` and `/tmp`.
- `SYSTEMROOT`: where cash finds Windows' own programs.
- `USERNAME`, `USERDOMAIN`, `COMPUTERNAME`: `whoami`, `logname`, `hostid`.
- `WT_SESSION`, `TERM_PROGRAM`: which terminal cash runs in.
