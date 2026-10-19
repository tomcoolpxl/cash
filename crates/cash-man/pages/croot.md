---
name: croot
summary: Pick a file or directory in a tree, and print it (Alt-E opens it on the command line).
kind: cash's shell commands
see: cd cdh prevd z keys
---

## Syntax

    croot [-d | -f] [DIR]

    croot           pick a directory below this one, and print it
    croot -f ~/src  a file or a directory, below ~/src
    Alt-E           the picker below the command line

## Examples

A directory picked into a variable, then a file:

<!-- parse only: opens a picker on the reader's screen -->

    dir=$(croot) && cd "$dir"
    code "$(croot -f ~/src)"

## How it works

croot is a file and directory picker. Alt-E opens it below the command line, which stays
on screen; `croot` opens it from a script and prints what you pick.

It shows a tree of the directory it starts in, several levels deep, trimmed to fit with
`… N more` lines.

### Keys

- `Up`, `Down`, `PgUp`, `PgDn`, `Home`, `End`: move.
- `Right`: make the selected directory the top of the tree. `Left`: go up to the parent;
  at a drive's top, the list of drives and your home directory.
- Typing: filter by name, fuzzily (`crt` finds `crates`), searching directories below
  the ones shown too. `Backspace` edits the filter.
- `Enter`: pick. `Ctrl-Enter`: pick and close.
- `Esc`: clear the filter; on an empty filter, close without a pick.
- `Alt-F`: directories only, or files too. `Alt-H`: the directories you go to most, best first,
  from the record `z` uses (see `help z`); while none are recorded, the directories this
  session was in, most recent first.
- `Alt-S`: newest first, with each entry's age (`3h`, `2d`), or back to names.
- `Alt-.`: show hidden entries. `Alt-I`: show entries a `.gitignore` leaves out.

### On the command line

What is on the line decides what the picker shows and what a pick does:

- an empty line, `cd` or `pushd`: directories only, and the pick runs at once (`cd PICK`);
- `rmdir` or `mkdir`: directories only;
- any other command: files and directories. The pick is inserted with a space after it, and
  the picker stays open for the next one, with each pick showing on the command line
  as you make it; `source` and `.` close it after one.
- a path already typed under the cursor: the picker starts in its directory, filtered by
  its last part (`cd ~/src/fo` starts in `~/src`, filtered by `fo`), and the pick
  replaces it.

A pick is written relative to the current directory when it is below it, as `~/…` under
your home directory, and as `C:/…` otherwise; it is quoted only when it needs quoting.

`bind '"\ec": cash-picker'` puts the picker on another key as well.

### Settings

- `CASH_PICKER_HEIGHT`: its height below the line, as a percentage (`50%`) or a
  number of rows (`15`); 40% when unset, and at least 8 rows.
- `LS_COLORS`: the colours of entries, as `ls` uses them.
- `CASH_PICKER_COLORS`: the colours of the picker's own parts, as SGR codes:
  `sel` (the selected line), `match` (the letters the filter matched), `frame` (the
  top line), `status` (the bottom line) and `dim` (tree lines and `… N more`). For
  example `CASH_PICKER_COLORS='sel=1;37;44:match=1;33'`.
- `NO_COLOR`: no colours.

### The command

`croot [-d|-f] [DIR]` opens the picker on the terminal, in DIR or the current directory,
showing directories only (`-d`, the default) or files and directories too (`-f`). It prints
what you pick on standard output, so `cd "$(croot)"` and `vim "$(croot -f)"` work. It
exits 1 when closed without a pick.
## Options

    -d    Directories only: the default.
    -f    Files and directories.
    DIR   Start there: the current directory without.

Exit status: 0 when something was picked; 1 when the picker closed without a pick.

