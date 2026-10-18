# The history picker, the directory jump, paths everywhere, and a guard on size and speed

Asked by the user on 2026-10-10, after 1.11.0: what they like most in cash is that it is
small and fast, that every way of writing a path works, and the prompt's helpers
(Alt-←/→, Alt-E). Four things were proposed in that spirit and all four asked for, then
`rm -rf` on junctions (DONE.md phase 34). This is the design; the choices the user made by pick list
on 2026-10-10 are marked, and recorded as D79 and D80 in [decisions.md](../decisions.md).

## 1. Ctrl-R: the history picker (D79)

Today Ctrl-R is Reedline's reverse search: a `(reverse-search)` prompt and one match at a
time. fzf's and atuin's Ctrl-R show a list instead, and Alt-E already draws one.

- **Where and how**: as Alt-E's picker (D73): inline below the line, the same height
  (`CASH_PICKER_HEIGHT`), colours (`CASH_PICKER_COLORS`) and moving keys, drawn by
  `cash-picker`'s terminal code; Esc on an empty filter closes it and leaves the line.
- **What it lists**: each command once, at its last use, newest first, with its age
  (`3m`, `2d`) as Alt-S shows ages. A command of several lines shows on one, its line
  ends marked.
- **Typing**: the same fuzzy filter as Alt-E (`nucleo-matcher`), on the whole command,
  best matches first and the newest of equals first. What was on the line when Ctrl-R
  was pressed is the filter it opens with.
- **Picking** (the user's pick): Enter puts the command on the line to edit (fzf), and
  Tab runs it at once.
- **The key** (the user's pick): Ctrl-R, with Bash's search still a Readline function `bind`
  can put back.
- **This directory only** (the user's pick): Bash's history file holds commands and times only.
  cash can keep its own record beside it, `%LOCALAPPDATA%\cash\history-directories`, a
  line per command run (time, directory, command), cut to `HISTFILESIZE` lines; Ctrl-R
  pressed again in the picker then shows only what ran in the current directory (atuin's
  scopes). Commands from before the record have no directory and show in the full list
  only.
- **Speed**: the first frame within 30 ms, as Alt-E's; the history is in memory
  already, and filtering 10,000 lines with nucleo takes a few milliseconds.
- Not in it: deleting entries, a preview, statistics, sync.

## 2. The directory jump (D80)

The directory history behind Alt-←/→ and `cdh` is a browser's back and forward: 25
directories, in memory, gone when the shell ends. zoxide's `z` is the usual answer for
"go to that directory I use"; cash's tools page names it as something to install.

- **The record**: every change of directory (`cd`, `pushd`, `popd`, `prevd`/`nextd`,
  Alt-←/→, a pick in Alt-E) counts a visit to the directory arrived in, kept across
  sessions in `%LOCALAPPDATA%\cash\directories` (local, not roaming: paths are this
  machine's). A directory's rank is zoxide's: its visits weighted by the last one's age
  (×4 within the hour, ×2 the day, ×½ the week, ×¼ older); when the visits add up
  past 10,000 all are scaled by 0.9 and those under 1 forgotten. A directory that is gone
  is skipped, and forgotten only after 90 days without a visit (changed while building,
  so an unplugged drive keeps its directories).
- **The command**, `z` (the user's pick): `z foo bar` goes to the best-ranked directory
  whose path holds `foo` and then `bar`, the last word in its last part, case aside
  (zoxide's rule); a word that is a directory (`..`, `C:/x`, `~/y`) is just `cd`'s;
  `z` alone goes home and `z -` back; no match says so with status 1. `z -l
  [WORDS]` lists the matches with their ranks; `z -i [WORDS]` opens Alt-E's picker on
  them. Tab after `z` completes from the record. If zoxide is installed and set up,
  its own `z` function comes before the builtin, as any function does.
- **Alt-E's Alt-H** shows the record, best first, typing filtering it, in place of the
  session's 25: Alt-E then Alt-H is the jump with the list in sight. Alt-←/→ stay the
  session's back and forward.

## 3. Every way of writing a path, everywhere

No new behaviour: a pass that each spelling cash takes (`C:\x`, `C:/x`, `/c/x`, `~`,
`~/x`, `\\server\share`, `//server/share`, `\\wsl$\…`, relative, with spaces) works in
each place a path is typed or written: arguments to builtins and to programs started,
redirections, globs, `cd`/`pushd`/`cdh`, Tab completion (completing each spelling in
that spelling), Alt-E's start directory and its pick, pasted lines. A table of places by
spellings, each cell a test; what fails is fixed in this phase.

## 4. A guard on size and speed

Size grew 35 MB (1.6.0) to 43 MB (1.10.0), all from new tools; startup stayed at about
25 ms over a bare process. Neither is checked anywhere.

- `cargo xtask perf`: the dist build's size, and the medians of 30 starts of
  `cash -c true` and of `cash.exe` running a bare `cmd /c exit`, the difference being
  cash's own start.
- `perf-budget.toml` in the repository holds the limits: the size of the last release
  plus 2 MB, and 60 ms of cash's own start (25 ms today; a GitHub runner is slower
  and noisier than this machine). A release that needs more raises the budget in its
  own commit, so growth is a choice written down.
- The release workflow, which builds the dist binary anyway, runs it before
  publishing, and writes the figures in the job's summary.

## 5. Then: `rm -rf` on junctions

`rm` is uutils' `uu_rm` 0.12.0: a junction inside a directory fails ("Permission denied"),
and a junction whose target was removed first fails too; both need `RemoveDirectoryW`
on the link, never following it. A near-upstream copy in `vendor/uutils`, as `uu_sort`
has, unless a newer uutils fixes it.
