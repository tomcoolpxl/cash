# `man`: cash's own manual and its browser

Status: designed, 2026-10-18; every question the design raised has the user's answer
(section 6), and section 7 is the order of the work. `man` is new and explicitly not
compatible with man(1); `help` stays as the short, printed view of the same pages; F1
keeps its own screen.

## 1. What it is

- `man NAME` opens a full-screen browser on NAME's page: the commands cash has (its
  builtins, its bundled tools) and cash's own topics. Nothing else: no roff, no
  sections 1 to 8, no `MANPATH`, no pages of programs cash did not build. `man` with no
  name opens the browser on its start page.
- `help NAME` prints the short part of the same page into the terminal and returns:
  what it is, the syntax, the common examples, and a line pointing to `man NAME`.
  `help` alone lists the commands. Bash's own options stay on `help`: `-s` (synopsis),
  `-d` (one-line description), `-m` (the page as a man page would print it).
- When `man`'s output is not a terminal (`man cd | grep`, `man cd > cd.txt`), it prints
  the whole page as plain text, without escapes, and returns.
- F1 at the prompt shows its one screen as now.

Why not compatible: man(1) is roff pages per section, found through `MANPATH`, for
programs installed on Unix. cash's commands are its own; its pages say what cash does,
including where that differs from the real command.

## 2. A page

Every page has the same sections in the same order, so a reader always knows where to
look. Empty sections are left out.

1. **Name and summary**: the command and one line saying what it is.
2. **Syntax**: the forms it is used in, readably, a line each with what that form does
   (`cd -          go back to the previous directory`), before any formal synopsis.
3. **Examples**: the commonly used ones, each a sentence saying what it does and the
   command. The most useful page section, so it comes before any explanation.
4. **How it works**: what a reader needs to understand to use it well.
5. **Options**: every option and operand, with exit statuses.
6. **Advanced**: the rest, for the reader who needs it.
7. **Different in cash**: where cash differs from the real command (Bash, GNU, BSD), and
   what cash adds; left out when there is nothing.
8. **See also**: the pages to read next, as links.

Samples, three depths of the same structure, for `cd` (a Bash builtin) and `sed` (a tool
cash took in): `research/man-samples/{cd,sed}-{A-short,B-standard,C-complete}.md`.

### Source format

Markdown, as the help catalogue is now (reviewable as text, embedded at build time),
with a header for what the browser and `help` need:

    ---
    name: cd
    summary: Change the shell's working directory.
    kind: Bash builtin
    see: pwd pushd prevd z croot
    ---

and `##` headings with the fixed names above. Only the markdown the pages need is
understood (headings, paragraphs, lists, indented code, tables, code spans, links to
other pages); the existing renderer (`helpdocs/render.rs`) already reads most of it.

### Every example is tested

Writing the `cd` samples found three `cd` bugs against Git Bash (`TODO.md`), and
checking the `sed` samples found two claims in the old pages that are no longer true.
So a page's examples are checked by a test, as the corpus checks its cases: each runs
under cash, and where the real command exists (Git Bash 5.3, GNU tools) under it too,
in a scratch directory with the files the page's test fixture sets up. A page cannot then
promise what cash does not do, and writing each tool's page is a bug hunt for that
tool. Examples that need what a test cannot have (a network share, a terminal) are
marked to be parsed only.

## 3. The browser

    ┌ Topics & commands ───┐┌ cd ─ Change the shell's working directory ───────────────┐
    │ ▸ Getting started    ││ SYNTAX                                                   │
    │   Paths on Windows   ││     cd                go to your home directory          │
    │   Line endings       ││     cd directory      go to directory                    │
    │   Keys               ││     cd -              go back to the previous directory  │
    │   Differences        ││                                                          │
    │ ──────────────────── ││ EXAMPLES                                                 │
    │   alias              ││   Go to a directory, by any spelling:                    │
    │   awk                ││       cd C:/src/cash                                     │
    │ ▸ cd                 ││   Go up one directory, or two:                           │
    │   chmod              ││       cd ..                                              │
    │   ...                ││ ...                                                      │
    └──────────────────────┘└──────────────────────────────────────────────────────────┘
     /find  Tab pane  ↑↓ PgUp PgDn  Enter open  Alt-← back  ? keys  Esc quit    3 / 12

- **Left pane**: one flat list, cash's topics first, a rule, then every command A to Z.
  The command or topic open is marked.
- **Right pane**: the page, wrapped to the pane's width; section headings stay
  recognisable as you scroll: the section's heading stays on the pane's top line.
- **Bottom line**: the keys that matter now, and where you are (`line 3 of 120`, or the
  search's `3 / 12`).
- **Narrow terminals** (under 100 columns, where two panes would leave the page under
  ~65): the page fills the screen; Tab shows the list in its place, Enter opens a page
  and goes back to it.

### Keys

As the user set them on trying the first build (2026-10-18): Up and Down choose a
topic and show it at once, no Enter; Page Up, Page Down, Home and End move in the
page, Up and Down with a modifier a line; no panes taking turns, no Tab.

| Key | Does |
|---|---|
| Up Down | the topic before or after: its page shows at once |
| PgUp PgDn, Home End | a screen up or down, the start or the end of the page |
| Shift+Up Shift+Down | a line up or down in the page (Windows Terminal takes Ctrl with them) |
| Ctrl+PgUp Ctrl+PgDn | the section before or after |
| letters, Backspace | narrow the list: by name, then summary, then the pages' text |
| Ctrl-F | find in the page as you type; F3, Shift-F3 and Enter go between matches |
| F1 | the keys, or the page again |
| Esc | leave the search, then the filter, then `man` |

### Search

Three searches, each where it belongs:

1. **The list filter**: typing in the left pane narrows it, fuzzy, by name and summary,
   with `croot`'s matcher (`nucleo-matcher`, already a dependency).
2. **Find in page**: `/` and a word; every match highlighted, the current one stronger,
   `n` / `N` between them, the count on the bottom line.
3. **Find everywhere**: `Ctrl-F` searches all pages' text (245 commands and the topics
   are a few hundred kilobytes, searched in memory in milliseconds); the left pane lists
   the pages that match, best first (name, then summary, then examples, then the rest),
   and opening one jumps to the first match.

`man -k WORD` (apropos) prints the commands whose name or summary matches, without the
browser.

### Colours

One palette, the one cash already uses (`helpdocs/render.rs`, the F1 screen, the
prompt's highlighting), so a command looks the same in `man`, in `help` and at the
prompt:

| What | Colour |
|---|---|
| section headings | bold blue (the prompt's accent) |
| what you type: commands, examples, code | green; a command's name bold green |
| topics | bold cyan |
| placeholders in syntax (`DIRECTORY`) | upper case, plain, as `--help` writes them |
| notes and marks | yellow |
| search matches | reverse video; the current one bold reverse |
| the list's selection | reverse video in the pane with focus, dim in the other |
| frames and the bottom line | dim |

Sixteen ANSI colours only, so the terminal's theme decides the shades, light or dark.
`NO_COLOR` and `--disable-color` give a plain page with reverse video for the
selection and matches.

## 4. Building it

### Crates

| Option | For | Against |
|---|---|---|
| Build on cash's own: `cash-picker`'s terminal layer and filter, `render.rs` | no new dependency; the same look and keys as `croot`; small | the drawing code to write: two panes, scrolling, wrapping |
| ratatui | a full layout and widget framework | a new framework beside cash's own drawing; size |
| termimad | markdown wrapping and scrolling on crossterm | its own styles; one pane; size |
| minus (pager) | a finished pager with search | one pane, no list, no links |
| md-tui (an app) | a markdown reader with search | an application, not a library |

The size budget decides much: a release stops when `cash.exe` grows past the last one
plus 2 MiB, and 1.26.0 is 49.6 MB of a 50.9 MB limit, about 1.3 MB of headroom.

### Storing the pages

Embedded at build time, as the catalogue is now, but compressed (cash carries gzip and
zstd): ~250 pages of a few kilobytes are ~1 MB as text, ~250 KB compressed, unpacked
once when `man` or `help` first needs them.

### Where it lives

A crate of its own, `cash-man`: the pages, their parser and renderer (moved from
`cash-builtins/src/helpdocs`), the browser, and the tests that check every builtin has a
page, every page's sections are in order, and every example runs.

## 5. The pages to write

245 commands and ~10 topics. The 85 pages and 10 topics there now are rewritten into
the new structure; the other ~160 start as drafts made from each command's `--help`
(syntax and options filled in, examples empty), marked as drafts in the list, and are
written in batches, most used first. Writing a batch runs its examples against Git Bash
and the real tools, and files what they find in `TODO.md`. The batches go most used
first (section 6).

## 6. Decided (the user, 2026-10-18)

- `man` is new, not compatible with man(1); `help` is the short, printed view of the same
  pages, under Bash's name and options; F1 keeps its screen.
- Pages are written at the complete depth (sample C) for every command.
- The headings are SYNTAX, EXAMPLES, HOW IT WORKS, OPTIONS, ADVANCED, DIFFERENT IN CASH,
  SEE ALSO.
- The colours are the ones cash already uses, as the mockup showed them.
- `man` with no name opens a "Getting started" topic.
- Every example runs as a test, under cash and, where it exists, under Git Bash or the
  real tool, and the two are compared.
- The browser is built on cash's own code: `cash-picker`'s terminal layer and filter, and
  the markdown renderer; no new framework.
- Esc leaves a search or filter being typed; otherwise it quits `man`.
- Examples are text: no key copies one or puts it on the command line.
- No mouse: the terminal's own selection keeps working.
- The word is "directory", never "folder", in every page.

- Placeholders are upper case (`cd [DIRECTORY]`), as `--help` and Bash's help write
  them: they read the same piped, copied, or without colour.
- The pages are written most-used first, in batches of about 20 (cd, ls, grep, find,
  sed, cp, mv, rm, cat, ...), each batch with its examples' tests and fixes for the
  bugs they find, and released when done.
- The section's heading stays on the top line while the page scrolls.
- Under 100 columns one pane fills the screen, Tab switching to the list (first set at
  80; at 80 the page pane had 45 columns beside the list).
- The list starts with "Getting started" and cash's topics, a rule, then every
  command A to Z, as the user asked from the start.

## 7. The order of the work

Each step ends with its tests passing and a commit; the first release with `man` comes
after step 6, so the browser ships with pages worth reading.

1. **`cash-man`, the pages' model**: the crate, the front matter and the fixed
   headings parsed into a page, and the checks: every builtin has a page or a draft,
   every page's sections are known and in order, every See-also names a page. The
   pages embedded compressed at build time, and the size of `cash.exe` measured
   against the budget.
2. **Rendering**: `render.rs` moved in and grown for the page model: sections,
   wrapping to a width, syntax lines in columns, examples, tables, links, plain text
   without colour.
3. **`help` on the new pages**: the short view (summary, syntax, examples, a pointer to
   `man NAME`), `help` alone listing the commands, Bash's `-s`, `-d` and `-m`; the
   current pages converted, so nothing is lost on the way.
4. **The browser**: the two panes on `cash-picker`'s terminal layer, the keys, the list
   filter, find in page, find everywhere, links with back and forward, the heading kept
   on the top line, one pane under ~80 columns, plain output when not a terminal, and
   `man -k`. Tested on a pseudo console as `croot` is.
5. **The examples' test**: each page's examples run in a scratch directory set up by
   the page's fixture, under cash and, where it exists, under Git Bash or the real tool,
   and the outputs and statuses compared; an example marked parse-only is parsed.
6. **The first batch**: about 20 pages at full depth, the most used (cd, ls, grep,
   find, sed, cp, mv, rm, cat, echo, printf, test, read, export, alias, history, jobs,
   kill, tar, ...), and drafts from `--help` for the rest, marked as drafts. What the
   examples find goes to `TODO.md` or is fixed.
7. **The next batches**, about 20 pages each, each released when done.
