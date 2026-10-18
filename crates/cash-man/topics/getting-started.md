---
title: Getting started
summary: What cash is, how to read this manual, and where to begin.
see: paths keys differences cd ls grep
---
cash is a shell for Windows that speaks Bash: the commands, the scripts and the
keys of Git Bash, with no MSYS2 or Cygwin under it. Paths are Windows paths in any
spelling, `C:/src`, `/c/src` or `C:\src`, and the tools you expect are inside it:
`ls`, `grep`, `sed`, `awk`, `find`, `tar`, `7z` and some 240 more.

## Reading this manual

The list on the left holds cash's topics first, then every command from A to Z.
Up and Down choose one, and its page shows at once; Page Up, Page Down, Home and
End move in the page, Ctrl with Up or Down a line. Typing narrows the list, by name,
summary or text. Ctrl-F finds a word in the page as you type, F3 goes to the next
match. F1 shows the keys; Esc leaves a search, then the filter, then the manual.

Every command's page has the same sections in the same order: what it is, its
syntax, examples, how it works, its options, advanced use, and what is different in
cash. A page marked as a draft is not written in full yet, and ends with the
command's own `--help`.

## Outside the manual

- `help NAME` prints the short form of a page into the terminal; `help` alone lists
  every command by kind.
- F1 at the prompt shows the keys of the command line on one screen.
- `NAME --help` prints a command's own options, as on Linux.

## Where to begin

- **Paths on Windows** (`paths`): which spellings cash accepts, and why `$PATH`
  looks different.
- **Keys** (`keys`): the command line's keys, history search and completion.
- **Differences** (`differences`): what is not as in Bash on Linux, and why.
- `cd`, `ls` and `grep`: the commands used most, a good first read.
