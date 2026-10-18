---
names: prevd nextd cdh
summary: Go back through the directory history.
summary nextd: Go forward through the directory history.
summary cdh: Choose a recent directory to go to.
kind: cash's shell commands
see: cd pushd dirs z keys
draft: yes
---

## Examples

    cd ~/src/app; cd /c/tools; prevd     # back in ~/src/app
    cdh                                  # pick from a numbered list

## How it works

cash remembers the directories you were in, as fish does: every `cd`, `pushd`, `popd` and
`cdh` records the directory it left, the last 25 of them.

- `prevd [N]` goes back N directories, one by default; `nextd [N]` goes forward again. Like
  a browser's back and forward, they move through the record without adding to it.
  `-l` lists it.
- `cdh` lists the recent directories, most recent as 1, and asks for a number. `cdh DIRECTORY`
  is `cd DIRECTORY`.

### Keys

On an empty line, `Alt-Left` is `prevd` and `Alt-Right` is `nextd`, and the prompt
redraws in the new directory. With text on the line they move a word, as usual. Windows
Terminal keeps Alt-arrows for split panes, so they reach cash in a tab that is not
split.
