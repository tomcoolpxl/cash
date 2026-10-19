---
names: unzip zipinfo
summary: List, test or extract a zip archive, with Info-ZIP's options.
summary zipinfo: List a zip archive in detail, as `ls -l` does: `unzip -Z`.
kind: Text and file tools
see: zip 7z tar
---

## Syntax

    unzip [OPTION]... ARCHIVE [MEMBER]... [-x MEMBER...] [-d DIRECTORY]
    zipinfo [OPTION]... ARCHIVE [MEMBER]...

    unzip site.zip               everything, into this directory
    unzip site.zip -d out        into out
    unzip -l site.zip            what is in it
    unzip -o site.zip 'css/*'    some members, replacing without asking
    unzip -p site.zip a.txt      a member to standard output
    zipinfo -l site.zip          modes, sizes, methods

## Examples

An archive listed, tested, and extracted elsewhere:

<!-- parse only: Git Bash has no zip to make the archive with -->

    zip -rq site.zip site
    unzip -l site.zip
    unzip -tq site.zip
    unzip -q site.zip -d out
    unzip -p site.zip site/a.txt
    zipinfo -1 site.zip

## How it works

`unzip ARCHIVE` extracts a zip archive into the current directory, `-d DIRECTORY` into
another; names after the archive choose members, `-x` names leave some out. `-l` and
`-v` list it, `-t` tests every member's data, `-p` writes members to standard output,
`-z` shows the comment. `zipinfo ARCHIVE` (or `unzip -Z`) lists it as `ls -l` does, from
`-1` (names only) to `-v` (every field of every record).

The options, messages, listings and exit statuses are Info-ZIP UnZip 6.00's and ZipInfo
3.00's. A wildcard in the archive's name (`unzip '*.zip'`) names every archive it
matches, with UnZip's tally after them.

It is all in Rust, in cash: stored, deflated, Deflate64, bzip2, LZMA, xz, zstd and PPMd
members, PKZIP 1's shrunk, reduced and imploded ones, Zip64 archives, split archives
(read from all their parts), and both the traditional encryption and WinZip's AES are
read; what Windows' Explorer, `tar.exe`, 7-Zip, WinZip and Info-ZIP's zip write, cash
reads. Info-ZIP's own unzip reads fewer of these.

## Options

unzip:

    -d DIRECTORY    Extract into DIRECTORY.
    -x MEMBER...    Leave these members out.
    -l              List the members.
    -v              List them in detail.
    -t              Test every member's data.
    -p              Members to standard output, nothing else.
    -c              Members to standard output, with their names.
    -z              The archive's comment.
    -o              Replace files without asking.
    -n              Never replace a file.
    -f              Freshen: only files that are there, if older.
    -u              Update: files that are older or not there.
    -j              Every member into one directory, no paths.
    -C              Match names case aside.
    -L              Names in lower case.
    -P PASSWORD     The password of an encrypted archive.
    -q, -qq         Quiet, quieter.
    -Z              List as zipinfo does.

zipinfo:

    -1              Names only.
    -s              Short, as ls -l does: the default.
    -m              Medium: with the compression ratio.
    -l              Long: with the compressed size.
    -v              Every field of every record.
    -h, -t          The header line, the totals line.
    -T              Times as sortable numbers.
    -z              The comment.

Exit status: Info-ZIP UnZip's: 0; 1 for a warning; 9 when the archive is not there; 11
when no member matches; others for errors.

## Different in cash

- Windows has no unzip command of its own; `tar.exe -xf` and `Expand-Archive` read zip
  archives with their own options.
- Extracting restores times and the read-only attribute (from a Unix mode's owner write
  bit, or MS-DOS's), not owners or other modes.
- A member's name Windows cannot hold (`:`, `"`, a trailing dot or space, `CON` and the
  like) is refused by name; `\` in a name is a directory separator. A drive and a leading
  `/` are removed with UnZip's warning, and `../` parts too, unless `-:`.
- A symbolic link is made where Windows allows one (Developer Mode, or an elevated
  shell), and written as a file holding its target where it does not.
- Tokenized, PKWARE DCL, Terse, LZ77 and WavPack members are skipped as UnZip skips a
  method it lacks.
- A password is asked for at the console when none is given with `-P`.
