---
title: Keys at the prompt
summary: The line editor's key bindings: history, completion, suggestions, Alt-arrows and the rest.
see: bind abbr prevd fc history config job-control
spec: D40 D59 D60 D61 D62
---
## The editor

The prompt is reedline in Emacs mode, with Readline's keys where cash adds them. There
is no vi mode: `set -o vi` is accepted and changes nothing. `bind` shows and changes
bindings, `bind -x` runs a command on a key, and `READLINE_LINE` and `READLINE_POINT`
work as in Bash.

## History and suggestions

- `Up`, `Down` (`Ctrl-P`, `Ctrl-N`): walk the history, or move in the completion menu.
- `Ctrl-R`: search the history as you type; `Ctrl-R` or `Up` again for older matches.
- A dimmed suggestion from the history follows the cursor. `Right`, `End`, `Ctrl-F` or
  `Ctrl-E` take all of it; `Ctrl-Right` or `Alt-F` one word.
- `Alt-.` (`Alt-_`): insert the previous command's last word; again for older ones.
- `Ctrl-X Ctrl-E`: edit the line in `$VISUAL`, or `$EDITOR`, or `vi`, then run it.
- `Alt-#`: comment the line out and keep it in the history.

## Completion

- `Tab`: complete; the first press inserts what the candidates share, the next opens
  the menu or moves through it. Completion ignores case and quotes what needs quoting
  (D40).
- `Shift-Tab`: move back through the menu. `Esc` closes it.

## Moving and editing

- `Home`/`Ctrl-A`, `End`/`Ctrl-E`: start and end of the line.
- `Ctrl-Left`/`Alt-B`, `Ctrl-Right`/`Alt-F`: a word back or forward.
- `Ctrl-W`: cut the word before the cursor. `Alt-D`: cut the word after it.
- `Ctrl-U`: cut to the start. `Ctrl-K`: cut to the end. `Ctrl-Y`: paste what was cut.
- `Ctrl-Backspace`, `Ctrl-Delete`: delete a word back or forward.
- `Alt-U`, `Alt-L`, `Alt-C`: upper-case, lower-case or capitalise a word.
- `Ctrl-T`: swap two characters.
- `Ctrl-Z` (also `Ctrl-X Ctrl-U`): undo. `Ctrl-G`: redo.
- `Alt-Enter`, `Shift-Enter`: a new line without running. An unfinished line (an open
  quote, `if` without `fi`) continues on Enter by itself.
- `Shift` with an arrow, `Home` or `End` selects; `Ctrl-Shift-A` selects all.

## Folders

On an empty line, `Alt-Left` runs `prevd` and `Alt-Right` runs `nextd`: back and
forward through the folders you were in, and the prompt redraws (D62). On a line with
text they move a word. Windows Terminal uses Alt-arrows for split panes, so they reach
cash only in a tab that is not split.

## Control keys

- `Enter`: run the line. Abbreviations (`abbr`) expand on Space and Enter (D60).
- `Ctrl-C`: abandon the line. While a program runs, it interrupts it (see `help job-control`).
- `Ctrl-D`: on an empty line, exit cash; otherwise delete the character under the cursor.
- `Ctrl-L`: clear the screen.
- `Ctrl-Z` while a program runs: suspend it (see `help job-control`).

## Display

- The line is highlighted as you type; a command that does not exist shows before you
  press Enter (D59). `syntax-highlighting = false` in the config file turns it off (see
  `help config`).
- `CASH_TRANSIENT_PS1`: when set, a finished command's prompt is redrawn as it, so the
  scrollback shows commands rather than prompts (D61). For example
  `CASH_TRANSIENT_PS1='\[\e[32m\]> \[\e[0m\]'`.
- `CASH_PS_ALT`: a prompt on the right side of the line.
