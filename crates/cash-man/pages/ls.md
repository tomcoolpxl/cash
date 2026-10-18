---
name: ls
summary: List directory contents, with colours, icons and a tree.
kind: Coreutils written for Windows
see: dir vdir tree stat dircolors find croot paths
---

## Syntax

    ls [OPTION]... [FILE]...

    ls                 list the current directory
    ls DIRECTORY       list a directory
    ls -l              a file a line: permissions, size, time, name
    ls -a              the hidden files too: names starting with a dot
    ls -lt             the newest first
    ls --tree          a directory and those below it, as a tree

## Examples

List the current directory, or only its Markdown files:

    ls
    ls *.md

A name a line, as a script reads it, the largest first:

    ls -1 -S

The names in a line, separated by commas:

    ls -m

The names in reverse order, or by extension:

    ls -r
    ls -X

A directory itself, not what it holds:

    ls -d .

A long listing with sizes people read, the newest first:

<!-- parse only: times and owners differ on every machine -->

    ls -lht

Everything, the hidden files too, directories first:

<!-- parse only: times and owners differ on every machine -->

    ls -la --group-directories-first

A project as a tree, two levels deep, with icons:

<!-- parse only: names a directory of the reader's own -->

    ls --tree --depth=2 src
    ls --tree --icons

## How it works

`ls` lists the names in each directory it is given, the current directory without
one, sorted by name. A file named is listed itself; a directory named has its
contents listed, under its name when there are several. Names starting with a dot are
left out unless `-a` or `-A` is given.

To a terminal it writes columns, in colour; to a pipe or a file one name a line,
plain, so that `ls | wc -l` counts files and `for f in $(ls)` reads them one by one
(though `for f in *` is the safer way: it keeps names with spaces whole).

### Sorting

By name unless an option says otherwise: `-t` by time, newest first; `-S` by size,
largest first; `-X` by extension; `-v` by version numbers inside names (`file2`
before `file10`); `-U` not at all, in the order the directory holds them. `-r` turns
any order around, and `--group-directories-first` puts the directories ahead.

### The long listing

`-l` gives a line a file:

    -rw-rw-r-- 1 me me 1234 Oct 18 12:00 notes.txt

The file's kind and permissions, its links, its owner and group, its size in bytes
(`-h` writes 1.2K, 34M), its time (the last change; `-u` the last read, `-c` the last
change to its name or attributes) and its name. A link shows where it leads after
`->`. The kind is `d` for a directory, `l` for a link, `-` for a file.

### Colours

With colour on, names take the colours `LS_COLORS` gives them, or `dircolors`'
defaults: directories blue, links cyan, programs green. In `-l` the other columns are
coloured too, as lsd colours them. `--color=never` turns them off; `--color=always`
keeps them in a pipe, for `less -R`.

## Options

    -a, --all               Do not ignore entries starting with . or files
                            both hidden and system, as Explorer hides them.
    -A, --almost-all        Like -a, but do not list implied . and ..
    --author                With -l, print the author of each file: its
                            owner, as on Linux.
    -b, --escape            Print C-style escapes for nongraphic characters.
    -B, --ignore-backups    Do not list implied entries ending with ~.
    -c                      With -lt: sort by, and show, the change time
                            (when the file's contents, name or attributes
                            last changed, NTFS's ChangeTime); with -l: show
                            it and sort by name; otherwise: sort by it,
                            newest first.
    -C                      List entries by columns.
    -d, --directory         List directories themselves, not their contents.
    -D, --dired             Generate output designed for Emacs' dired mode.
    -F, --classify          Append indicator (one of */@) to entries.
    --file-type             Likewise, except do not append '*'.
    --format=WORD           Across -x, commas -m, horizontal -x, long -l,
                            single-column -1, verbose -l, vertical -C.
    --full-time             Like -l --time-style=full-iso.
    -g                      Like -l, but do not list the owner.
    -G, --no-group          In a long listing, do not print the group.
    -h, --human-readable    With -l and -s, print sizes like 1K 234M 2G.
    --si                    Likewise, but use powers of 1000 not 1024.
    --block-size=SIZE       With -l, scale sizes by SIZE when printing them,
                            and -s's blocks: a number, a unit (K, M, G, T,
                            P, E; KB for 1000, KiB for 1024, shown after
                            each size when there is no number) or both;
                            human-readable, si; LS_BLOCK_SIZE or BLOCK_SIZE
                            give it otherwise.
    -H, --dereference-command-line
                            Follow symbolic links listed on the command
                            line.
    --dereference-command-line-symlink-to-dir
                            Follow each command line symbolic link that
                            points to a directory.
    --hide=PATTERN          Do not list implied entries matching shell
                            PATTERN (overridden by -a or -A)
    --hyperlink[=WHEN]      Hyperlink file names: always, auto (when the
                            output is a terminal), never (the default);
                            file:///C:/… URLs, with no host, which Windows
                            Terminal opens.
    -i, --inode             Print the index number of each file (NTFS's file
                            ID, as Git Bash shows it)
    -I, --ignore=PATTERN    Do not list implied entries matching shell
                            PATTERN.
    -k, --kibibytes         -s's blocks and the totals in 1024 bytes.
    --indicator-style=WORD  Append an indicator with style WORD to names:
                            none (the default), slash (-p), file-type
                            (--file-type), classify (-F)
    -l                      Use a long listing format.
    -L, --dereference       Show what a link names rather than the link.
    -m                      Fill the width with names separated by commas.
    -n, --numeric-uid-gid   Like -l, but list numeric user and group IDs:
                            the accounts' RIDs, as id and stat give them.
    -N, --literal           Print names as they are, without quoting.
    -o                      Like -l, but do not list the group.
    -p, --indicator-style=slash
                            Append / to directories.
    -q, --hide-control-chars
                            Print ? instead of nongraphic characters.
    --show-control-chars    Show nongraphic characters as they are (the
                            default unless the output is a terminal)
    -Q, --quote-name        Enclose names in double quotes.
    --quoting-style=WORD    How names are quoted: literal, locale, shell,
                            shell-always, shell-escape, shell-escape-always,
                            c, c-maybe, escape, clocale; QUOTING_STYLE gives
                            it otherwise, and on a terminal it is shell-
                            escape.
    -r, --reverse           Reverse order while sorting.
    -R, --recursive         List subdirectories recursively.
    -s, --size              Print the space each file takes, in blocks.
    -S                      Sort by file size, largest first.
    -t                      Sort by time, newest first; see --time.
    -T, --tabsize=COLS      Checked as GNU ls checks it; cash lines columns
                            up with spaces, never tabs, whatever COLS is.
    --time=WORD             The time -l shows and -t sorts by: atime,
                            access, use (-u); ctime, status (-c); mtime,
                            modification (the default); birth, creation;
                            without -l, any but mtime sorts by itself.
    -u                      Like -c, with the time it was last read.
    -U                      Do not sort; list entries in directory order.
    -v                      Natural sort of (version) numbers within text.
    -w, --width=COLS        Set the output width to COLS; 0 means no limit.
    -x                      List entries by lines instead of by columns.
    -X                      Sort alphabetically by entry extension.
    -Z, --context           Print any security context of each file: ?, as
                            GNU ls prints it for a file with none.
    --zero                  End each output line with NUL, not newline.
    -1                      List one file per line.
    --sort=WORD             Sort by WORD instead of name: none (-U), size
                            (-S), time (-t), version (-v), extension (-X),
                            name.
    --group-directories-first
                            Group directories before files.
    --time-style=STYLE      How -l shows times: full-iso, long-iso, iso,
                            locale (the default), or +FORMAT, strftime's,
                            FORMAT1 and FORMAT2 on two lines for old and
                            recent files; posix-STYLE is the default here;
                            TIME_STYLE gives it when the option is not
                            given.
    --color[=WHEN]          Color the output: always, auto (the default),
                            never; names take LS_COLORS's colors, or
                            dircolors' defaults; with -l, the other columns
                            take lsd's.
    --icons[=WHEN]          Show an icon before each name: always, auto
                            (when the output is a terminal, the default),
                            never.
    --icons-theme=THEME     Auto (the default: Nerd Font glyphs when Windows
                            Terminal draws this tab in a Nerd Font, no icons
                            anywhere else) or fancy (the glyphs regardless)
    --attributes            Show Windows' attributes as lsd does: d (or .),
                            archive, read-only, hidden, system (`.a-h-`)
    --count-links           With -l, count a directory's links as Linux has
                            them, 2 and one per subdirectory, reading each
                            one; without it a directory shows 1, as in Git
                            Bash.
    --tree                  List the directories as a tree.
    --depth=NUM             Descend at most NUM levels, with --tree or -R.
    --help                  Display this help and exit.

`FILE` is a file or a directory to list; without one, the current directory.

Exit status: 0 when all went well; 1 for a file or directory that could not be read
but `ls` went on; 2 for a bad option or a name that is not there.

## Advanced

### Trees

`--tree` lists a directory and every directory below it as a tree, lsd's way;
`--depth=N` stops N levels down. With `-l` each line is a long one.

### Times and sizes

`--time-style` chooses how `-l` writes times: `full-iso`, `long-iso`, `iso`, or a
format of your own, `+%Y-%m-%d %H:%M`. `--block-size` scales sizes: `--block-size=M`
shows them in mebibytes, `--si` in powers of 1000.

### For scripts

`-1`, `--zero` (each name ended by a NUL), `-N` (names as they are, unquoted) and
`--quoting-style` make output a program reads safely. For a list of files to work on,
`find` or a glob is better than parsing `ls`.

## Different in cash

- It is cash's own `ls`, written for Windows, with GNU's options and lsd's additions:
  colours in every column, `--icons`, `--tree` and `--depth`, `--attributes`.
- It colours when it writes to a terminal, with no alias needed; a pipe or a file gets
  plain names.
- `-l`'s `w` is the access list's answer: whether you may write the file (or create
  files in the directory). `x` is the file's kind, as `test -x` decides it: a program,
  a script with `#!`, a directory.
- Files that are both hidden and system (the old `My Documents` junctions,
  `NTUSER.DAT`'s logs) are left out without `-a` or `-A`, as Explorer leaves them out.
  A file that is hidden only is listed, as a dot file is not.
- `--attributes` shows the Windows attributes: archive, read-only, hidden, system.
- A directory's link count in `-l` is 1, as in Git Bash: Linux's 2 and one per
  directory in it would mean reading every directory listed. `--count-links` counts
  them anyway; `stat` always counts the one asked about.
- `--icons` draws Nerd Font icons when Windows Terminal draws the tab in a Nerd Font;
  `--icons-theme=fancy` draws them anywhere.
- The group column names the group Windows records for the file, where Git Bash
  often shows a number (`197609`). `-i` prints NTFS's file ID, as Git Bash does, and
  `-n` the accounts' RIDs, as `id`
  and `stat` give them.
