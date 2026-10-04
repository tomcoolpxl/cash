---
see: differences
---
## Description

`help` shows what cash knows about its builtins and about itself.

- `help` alone lists every builtin by kind, each with a line about it.
- `help NAME` shows a builtin's page: what it does, its Windows notes, and its own
  options. `NAME` may be a pattern: `help 'c*'`.
- `help topics` lists the topics, such as `paths`, `crlf`, `keys` and `differences`;
  `help TOPIC` shows one.
- `help search WORD` searches every name, summary, page and topic, and shows the lines
  that matched.
- `-d` gives the one-line summary, `-s` the usage line, and `-m` the page (as `help NAME`
  does).

From PowerShell or cmd, `cash help ...` says the same, unless a file named `help` is in
the current folder, which then runs as a script.

## Windows notes

Builtins marked `+` in the list hide a Windows program of the same name in System32
(`find.exe`, `sort.exe`, `more.com`, ...): inside cash the builtin wins. Run the Windows
one by its path: `"$SYSTEMROOT/System32/find.exe"`. `help.exe` itself is one of them.
