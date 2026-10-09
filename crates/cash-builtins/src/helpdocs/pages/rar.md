---
names: rar unrar
see: 7z zip tar
---
## Description

`rar l ARCHIVE` lists a RAR archive (`lt` with every field, `lta` with the service
records too, `lb` names alone; `v`, `vt`, `vta` and `vb` the same, with `-v` adding the
volumes after it); `rar t` tests it; `rar x` extracts it with its folders and `rar e`
without them, into the current folder or the folder named last with a trailing `\` or
`/`, or `-op`; `rar p` prints files to standard output. Names after the archive choose
files, `-x` and `-n` leave some out or keep only some, `@list` reads them from a file.
`unrar` has these commands alone.

`rar a ARCHIVE NAMES` adds files and folders, making the archive if need be (`.rar` is
added to a name without an extension); `rar u` adds what is new or newer, `rar f` only
what is newer than its archived copy, `rar m` and `rar mf` delete what was archived,
`rar d` deletes files from the archive, and erases it when nothing is left. A folder
named is added with all it holds, `-r-` takes it alone; a wildcard takes files, and with
`-r` every folder below and the folders it matches. `-ep`, `-ep1` to `-ep4` and `-ap`
choose the names stored, `-ed`, `-e`, `-sl`, `-sm`, `-ta`, `-tb`, `-tn` and `-to` what
is taken. `-m0` to `-m5` choose the level, `-s` a solid archive, `-htb` BLAKE2
checksums for CRC32, `-qo` the quick-open record, `-rr` a recovery record,
`-k` locks the archive, `-z` adds a comment, `-p` and `-hp` encrypt the data and the
names, `-v` cuts the archive into volumes (`NAME.part1.rar` and on), `-t` tests what was
written, `-df` deletes what was added, `-ts` keeps the creation and access times too.

`rar c` adds a comment, from `-z`'s file or standard input; `rar cw` writes it to a file
or standard output. `rar rn ARCHIVE OLD NEW…` renames files (a folder with what it
holds; `*` and `?` in OLD and NEW for patterns such as `'src/*.txt' 'src/*.bak'`).
`rar k` locks an archive against change, `rar rr[N]` gives it a recovery record of N%
(3 by default), and `rar ch` changes it by switches (`-cl`, `-cu`, `-z`, `-k`, `-tl`).
Each copies the archive's files as they are. `rar i=TEXT` looks for a string in the
archived files and shows where it is (`ic=` case-sensitive, `ih=` hexadecimal bytes,
`it=` in UTF-8 and UTF-16 too). `rar r` repairs a damaged archive: with a recovery
record, what it protects is mended into `fixed.NAME`; without one, every file whose
header is whole is copied into `rebuilt.NAME`, damaged data and all; a folder named
last with a trailing `\` or `/` takes the result.

The commands, switches, messages, listings and exit codes are WinRAR 7.23's console
`Rar.exe` and `UnRAR.exe`'s, learned by running them and from their manual: switches
anywhere until `--`, `RARINISWITCHES` and `%APPDATA%\WinRAR\rar.ini` for default
switches (`-cfg-` reads neither), the overwrite question and `-o+`, `-o-`, `-or`, `-y`,
the password asked for each archive or file that needs one, `-id` and `-inul` for
quieter messages, `-ierr` to send them to standard error.

It is all in Rust, in cash. It reads RAR archives from 1.3 to 7, with their volumes
(`name.part1.rar`, or `name.rar`, `name.r00` and on), solid streams, comments, links,
recovery records and `.rev` volumes, and encrypted data and headers; damaged headers are
read as far as they go. It writes RAR 5: a stored archive (`-m0`) is WinRAR's byte for
byte, volumes too; a compressed one holds the same files, packed by other code. An
update copies the files already archived as they are, not packed again, as WinRAR does.

## Windows notes

- Lines end in LF and names show `/`, where `Rar.exe` writes CRLF and `\`. Masks take
  `\` and `/` alike; `Rar.exe` on Windows matches none with `/`.
- Names and messages are UTF-8 wherever they go; `Rar.exe` writes the ANSI code page into
  files and pipes unless told `-scfr`.
- Times are shown in the zone the shell's `TZ` names, else in Windows' own.
- `rar.ini` and `rarfiles.lst` are read from `%APPDATA%\WinRAR`, not from a folder of
  `Rar.exe`'s own. A compressed solid archive's files are sorted by extension, or by
  `rarfiles.lst` when there is one; WinRAR 7.23 reads that list beside `Rar.exe` only.
- A new archive is RAR 5: `-ma4` is an unknown option, as in WinRAR 7.23. An update of
  a RAR 4 archive keeps it RAR 4, as WinRAR's does.
- Self-extracting archives (`s`, `-sfx`) and recovery volumes (`rv`) are not made.
  `rc` is not in cash's rar yet. A volume set is not changed by `c` or `k`, as it is
  by WinRAR. `r` gives an archive whose main header is damaged a plain one, where
  WinRAR asks whether to mark it solid.
- A WinRAR installed by Scoop or by its installer stays reachable by its path, or after
  `enable -n rar`.

## Examples

```
rar l archive.rar
rar x archive.rar out/            # with its folders, into out
rar e archive.rar '*.txt'          # the text files, without their folders
rar p archive.rar notes.txt | less
rar t -psecret secret.rar
rar a backup.rar docs              # docs and all in it
rar u backup.rar docs              # only what is new or changed since
rar a -m0 -htb site.rar public     # stored, with BLAKE2 checksums
rar a -hp secret.rar notes         # asks for the password; names hidden too
rar a -v100m big.rar media         # big.part1.rar, .part2.rar, ... of 100 MiB
rar x big.part1.rar                # all the volumes
rar d backup.rar 'docs/*.tmp'
unrar x download.rar
```
