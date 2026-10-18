---
name: z
summary: Jump to a directory you often go to, from a word of its path.
kind: cash's shell commands
see: cd prevd croot keys vars
draft: yes
---

## Examples

    z cash     # the directory whose last part holds "cash" you go to most
    z src api  # "src", then "api" in the last part
    z -l       # every recorded directory, with its score
    z -i doc   # pick from the directories matching "doc"

## How it works

`z` jumps to a directory you go to often, from a word or two of its path, as zoxide's `z`
does: `z cash` goes to `~/github/cash` once you have been there.

An interactive cash records each directory it shows a prompt in, across sessions, in
`%LOCALAPPDATA%\cash\directories`. A directory's rank is its visits weighted by how recent the
last one was: ×4 within the hour, ×2 within the day, ×½ within the week, ×¼ after.
When the visits add up past 10,000 they are all scaled down, and directories that drop
below one visit are forgotten.

- `z WORDS` goes to the best-ranked directory whose path holds each word in turn, case
  aside, the last word in its last part: `z src api` finds `C:/src/shop/api-server`,
  not `C:/src/api/docs`. The directory you are in is left out, so `z` again goes to the
  next best.
- `z` alone goes home, `z -` back to the last directory, and `z DIRECTORY`, a directory that is
  there, is `cd DIRECTORY`.
- `z -l [WORDS]` lists the matches with their scores, best first.
- `z -i [WORDS]` lists them below the line to pick from, typing filtering them; Enter
  goes to the one picked, Esc closes.
- Tab after `z WORDS` offers the matching directories, best first, and puts the one chosen
  on the line in place of the words.

A directory that is gone is skipped, and forgotten once it has gone 90 days without a
visit, so a drive unplugged for a while keeps its directories. `z` says when nothing
matches, and exits 1.

In `Alt-E`'s picker, `Alt-H` lists the same directories, best first (see `help croot`).

## Different in cash

- A script can use `z` too: it reads the record, and adds nothing to it.
- `CASH_NO_RECORDS`, set and not empty, keeps no record (see `help vars`).
- A `z` function, such as zoxide's own once `zoxide init bash` is sourced, comes before
  this builtin, as any function does.
