---
title: Keys at the prompt
summary: The line editor's key bindings: history, completion, suggestions, Alt-arrows and the rest.
see: bind abbr prevd fc history config job-control
---
## The editor

The prompt is reedline in Emacs mode, with Readline's keys where cash adds them.
`set -o vi` switches to vi keys at the next prompt (insert mode first, Esc for normal
mode), and `set -o emacs` back; Tab completion and the keys `bind` adds work in vi's
insert mode too. `bind` shows and changes
bindings, `bind -x` runs a command on a key, and `READLINE_LINE` and `READLINE_POINT`
work as in Bash.

## History and suggestions

- `Up`, `Down` (`Ctrl-P`, `Ctrl-N`): walk the history, or move in the completion menu.
- `Ctrl-R`: pick a command from the history, in a list below the line. Each command
  shows once, newest first, with its age; typing filters it, and what was on the line
  is the filter it opens with. `Enter` puts the command on the line to edit, `Tab` runs
  it. `Ctrl-R` in the list switches to what ran in this folder and back. `Esc` clears
  the filter, then closes the list. Reedline's search as you type is the Readline
  function `reverse-search-history`: `bind '"\C-r": reverse-search-history'` puts it
  back on `Ctrl-R`.
- A dimmed suggestion from the history follows the cursor. `Right`, `End`, `Ctrl-F` or
  `Ctrl-E` take all of it; `Ctrl-Right` or `Alt-F` one word.
- `Alt-.` (`Alt-_`): insert the previous command's last word; again for older ones.
- `Ctrl-X Ctrl-E`: edit the line in `$VISUAL`, or `$EDITOR`, or `vi`, then run it.
- `Alt-#`: comment the line out and keep it in the history.

## Completion

- `Tab`: complete; the first press inserts what the candidates share, the next opens
  the menu or moves through it. Completion ignores case and quotes what needs quoting.
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

`Alt-E` opens croot, a file and folder picker, below the line: on an empty line or after
`cd` it goes to the folder you pick, after any other command it inserts the path (see
`help croot`).

On an empty line, `Alt-Left` runs `prevd` and `Alt-Right` runs `nextd`: back and
forward through the folders you were in, and the prompt redraws. On a line with
text they move a word. Windows Terminal uses Alt-arrows for split panes, so they reach
cash only in a tab that is not split.

## Control keys

- `Enter`: run the line. Abbreviations (`abbr`) expand on Space and Enter.
- `Ctrl-C`: abandon the line. While a program runs, it interrupts it (see `help job-control`).
- `Ctrl-D`: on an empty line, exit cash; otherwise delete the character under the cursor.
- `Ctrl-L`: clear the screen.
- `Ctrl-Z` while a program runs: suspend it (see `help job-control`).
- `F1`: one screen of help on paths and keys for someone who knows Bash; `F1`, `Esc` or
  `q` closes it. It is the Readline function `cash-help`, which `bind` can move.

## Display

- The line is highlighted as you type; a command that does not exist shows before you
  press Enter. `syntax-highlighting = false` in the config file turns it off (see
  `help config`).
- `CASH_TRANSIENT_PS1`: when set, a finished command's prompt is redrawn as it, so the
  scrollback shows commands rather than prompts. For example
  `CASH_TRANSIENT_PS1='\[\e[32m\]> \[\e[0m\]'`.
- `CASH_PS_ALT`: a prompt on the right side of the line.
