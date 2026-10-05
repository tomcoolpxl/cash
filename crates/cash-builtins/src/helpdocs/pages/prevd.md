---
names: prevd nextd cdh
see: cd pushd dirs keys
---
## Description

cash remembers the folders you were in, as fish does: every `cd`, `pushd`, `popd` and
`cdh` records the folder it left, the last 25 of them.

- `prevd [N]` goes back N folders, one by default; `nextd [N]` goes forward again. Like
  a browser's back and forward, they move through the record without adding to it.
  `-l` lists it.
- `cdh` lists the recent folders, most recent as 1, and asks for a number. `cdh FOLDER`
  is `cd FOLDER`.

## Keys

On an empty line, `Alt-Left` is `prevd` and `Alt-Right` is `nextd`, and the prompt
redraws in the new folder. With text on the line they move a word, as usual. Windows
Terminal keeps Alt-arrows for split panes, so they reach cash in a tab that is not
split.

## Examples

```
cd ~/src/app; cd /c/tools; prevd     # back in ~/src/app
cdh                                  # pick from a numbered list
```
