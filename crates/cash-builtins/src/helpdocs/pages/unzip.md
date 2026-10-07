---
names: unzip zipinfo
see: zip tar
---
## Description

`unzip ARCHIVE` extracts a zip archive into the current folder, `-d FOLDER` into
another; names after the archive choose members, `-x` names leave some out. `-l` and
`-v` list it, `-t` tests every member's data, `-p` writes members to standard output,
`-z` shows the comment. `zipinfo ARCHIVE` (or `unzip -Z`) lists it as `ls -l` does, from
`-1` (names only) to `-v` (every field of every record).

The options, messages, listings and exit statuses are Info-ZIP UnZip 6.00's and ZipInfo
3.00's. That takes in `-o`, `-n`, `-f`, `-u`, `-j`, `-C`, `-L`, `-W`, `-q`, `-qq`, `-c`,
`-D`, `-P`, `-T`, the overwrite question, `UNZIP` and `ZIPINFO`, and zipinfo's `-s`, `-m`,
`-l`, `-h`, `-t`, `-T` and `-z`. A wildcard in the archive's name (`unzip '*.zip'`) names
every archive it matches, with UnZip's tally after them.

It is all in Rust, in cash: stored, deflated, Deflate64, bzip2, LZMA, xz, zstd and PPMd
members, PKZIP 1's shrunk, reduced and imploded ones, Zip64 archives, split archives
(read from all their parts), and both the traditional encryption and WinZip's AES are
read; what Windows' Explorer, `tar.exe`, 7-Zip, WinZip and Info-ZIP's zip write, cash
reads. Info-ZIP's own unzip reads fewer of these.

## Windows notes

- Windows has no unzip command of its own; `tar.exe -xf` and `Expand-Archive` read zip
  archives with their own options.
- Extracting restores times and the read-only attribute (from a Unix mode's owner write
  bit, or MS-DOS's), not owners or other modes.
- A member's name Windows cannot hold (`:`, `"`, a trailing dot or space, `CON` and the
  like) is refused by name; `\` in a name is a folder separator. A drive and a leading
  `/` are removed with UnZip's warning, and `../` parts too, unless `-:`.
- A symbolic link is made where Windows allows one (Developer Mode, or an elevated
  shell), and written as a file holding its target where it does not.
- Tokenized, PKWARE DCL, Terse, LZ77 and WavPack members are skipped as UnZip skips a
  method it lacks.
- A password is asked for at the console when none is given with `-P`.

## Examples

```
unzip release.zip -d /tmp/release
unzip -l release.zip               # what is in it
unzip -o site.zip 'site/css/*'     # some members, replacing without asking
unzip -p data.zip report.csv | head
zipinfo -l release.zip             # modes, sizes, methods
```
