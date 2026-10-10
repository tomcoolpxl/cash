---
see: cd prevd croot keys vars
---
## Description

`z` jumps to a folder you go to often, from a word or two of its path, as zoxide's `z`
does: `z cash` goes to `~/github/cash` once you have been there.

An interactive cash records each folder it shows a prompt in, across sessions, in
`%LOCALAPPDATA%\cash\folders`. A folder's rank is its visits weighted by how recent the
last one was: ×4 within the hour, ×2 within the day, ×½ within the week, ×¼ after.
When the visits add up past 10,000 they are all scaled down, and folders that drop
below one visit are forgotten.

- `z WORDS` goes to the best-ranked folder whose path holds each word in turn, case
  aside, the last word in its last part: `z src api` finds `C:/src/shop/api-server`,
  not `C:/src/api/docs`. The folder you are in is left out, so `z` again goes to the
  next best.
- `z` alone goes home, `z -` back to the last folder, and `z FOLDER`, a folder that is
  there, is `cd FOLDER`.
- `z -l [WORDS]` lists the matches with their scores, best first.
- `z -i [WORDS]` lists them below the line to pick from, typing filtering them; Enter
  goes to the one picked, Esc closes.
- Tab after `z WORDS` offers the matching folders, best first, and puts the one chosen
  on the line in place of the words.

A folder that is gone is skipped, and forgotten once it has gone 90 days without a
visit, so a drive unplugged for a while keeps its folders. `z` says when nothing
matches, and exits 1.

In `Alt-E`'s picker, `Alt-H` lists the same folders, best first (see `help croot`).

## Windows notes

- A script can use `z` too: it reads the record, and adds nothing to it.
- `CASH_NO_RECORDS`, set and not empty, keeps no record (see `help vars`).
- A `z` function, such as zoxide's own once `zoxide init bash` is sourced, comes before
  this builtin, as any function does.

## Examples

```
z cash              # the folder whose last part holds "cash" you go to most
z src api           # "src", then "api" in the last part
z -l                # every recorded folder, with its score
z -i doc            # pick from the folders matching "doc"
```
