---
name: man
summary: Read cash's manual: a page for every command and topic, in a browser.
kind: cash's shell commands
see: help getting-started keys differences
---

## Syntax

    man [NAME]
    man -k WORD
    man -w [NAME]

    man               open the manual on Getting started
    man NAME          open the page of a command or a topic
    man NAME | less   print the page as text, for a pipe or a file
    man -k WORD       list the commands and topics whose summary holds WORD

## Examples

Open the page of a command, or of a topic:

    man tar
    man paths

Find a command when you know what it does, not its name:

    man -k archive
    man -k 'line endings'

Keep a page, or search it with the tools you know:

    man sed > sed.txt
    man grep | grep -- --include

## How it works

`man` opens a full-screen browser: cash's topics and every command on the left, the
page of the one chosen on the right. Up and Down choose a topic, and its page shows at
once; nothing is opened. Typing filters the list, by name, then summary, then the
pages' text; what is typed shows at the list's top. In a terminal under 100 columns
the page fills the screen, and Up and Down still choose.

Every command's page has the same sections in the same order: Syntax, Examples, How
it works, Options, Advanced, Different in cash, See also. The page's first line, its
name and what it does, stays on top while you scroll.

### Keys

    Up Down                the topic before or after: its page shows at once
    PgUp PgDn              a screen up or down in the page
    Home End               the start or the end of the page
    Shift+Up Shift+Down    a line up or down in the page
    Ctrl+PgUp Ctrl+PgDn    the section before or after
    letters                narrow the list: by name, summary, then text
    Backspace              take a letter off what is typed
    Ctrl-F                 find in the page, as you type
    F3 Shift-F3            the next or the previous match; Enter too
    F1                     the keys, or the page again
    Esc                    leave the search, then the filter, then man

When `man`'s output is not a terminal, it prints the page as plain text, wrapped to
`$COLUMNS` (80 without it), and returns. `help NAME` prints the short form of a page,
its syntax and examples, into the terminal.

## Options

    -k WORD    Print each command and topic whose name or summary holds
               WORD, case aside, with its summary.
    -w, --path, --where
               Print where the pages are: nothing, as cash's are built in.
               Scripts such as direnv's put `man -w` before a MANPATH of
               their own; what they get ends in `:`, which keeps the
               directories of a man(1) run by its path.
    NAME       A command, or a topic by its name or title. Without one, the
               topic Getting started.

Exit status: 0 when the page was shown, or for `-w` when there is one; 1 when there
is no page for NAME, or `-k` found nothing; 2 for a bad option.

## Different in cash

- It is not man(1): there are no roff pages, no sections 1 to 8 and no `MANPATH`. The
  pages are cash's own, one for each command cash has and for its topics, and say
  what cash does, where it differs from the real command too.
- A man found on `PATH`, such as Git for Windows' `/usr/bin/man`, is not run: the
  builtin comes first, as builtins do. Run that one by its path.
- A page not written in full yet is marked as a draft, and ends with the command's
  own `--help`.
