---
name: zip
summary: Pack files into a zip archive, or update or delete its members, with Info-ZIP's options.
kind: Text and file tools
see: unzip 7z tar gzip
---

## Syntax

    zip [OPTION]... ARCHIVE [FILE]...

    zip -r site.zip site/            a directory, whole
    zip -u site.zip site/index.html  only what is new or changed
    zip -d site.zip 'site/tmp/*'     members out
    zip -9 -x '*.log' logs.zip logs/*    smallest, some left out
    zip -r - src > src.zip           to standard output

## Examples

A directory archived, listed, and a member taken out:

<!-- parse only: Git Bash has no zip to compare with -->

    mkdir -p site; printf 'a\n' > site/a.txt; printf 'b\n' > site/b.log
    zip -rq site.zip site
    unzip -l site.zip
    zip -d site.zip 'site/*.log'
    zip -rq -e secret.zip private/

## How it works

`zip ARCHIVE FILE...` adds files to a zip archive, making it when there is none and
replacing members already in it; `-r` takes directories whole. `-u` adds only what is new or
changed, `-f` replaces only what changed, `-d` deletes members and `-FS` makes the
archive hold exactly the files named. `.zip` is added to an archive's name that has no
suffix. `-` as the archive writes it to standard output; `-` as a file reads standard
input.

The options, messages and exit statuses are Info-ZIP zip 3.0's. `-s SIZE` writes a
split archive (`NAME.z01`, `NAME.z02`, ... `NAME.zip`), `-s 0 SPLIT.zip --out ONE.zip`
makes one archive of it again, and `-F` or `-FF` with `--out` salvages a damaged one.
Stored archives without extra fields (`-0 -X`) are zip's byte for byte, split ones too.

It is all in Rust, in cash. What cash writes, Windows' Explorer, `tar.exe`, 7-Zip and
Info-ZIP's unzip read, with Unix modes and times.

## Options

    -r                  Directories, whole.
    -u                  Add what is new or changed.
    -f                  Replace what changed; add nothing.
    -d                  Delete the members named.
    -FS                 The archive holds exactly the files named.
    -m                  Move: remove the files once archived.
    -0 ... -9           Store, to smallest: 6 without.
    -Z METHOD           store, deflate or bzip2.
    -j                  Names without their directories.
    -D                  No entries for directories.
    -x PATTERN...       Leave these out.
    -i PATTERN...       Only these.
    -@                  Read the names from standard input.
    -n SUFFIXES         Store these without compressing: .zip:.png
    -e                  Encrypt, the password asked for.
    -P PASSWORD         Encrypt with PASSWORD.
    -z                  An archive comment, from standard input.
    -c                  A comment for each member.
    -l, -ll             LF to CRLF, CRLF to LF, in text.
    -X                  No Unix ids or times in extra fields.
    -y                  Store a link as a link.
    -t DATE, -tt DATE   Only files changed on or after, before, DATE.
    -o                  The archive's time the newest member's.
    -T                  Test the archive once written.
    -s SIZE             Split into parts of SIZE: 100m.
    -sf                 List the files that would go in.
    --out FILE          Write the result to FILE.
    -F, -FF             Salvage a damaged archive, with --out.
    -q                  Quiet.
    -v                  Say more.

Exit status: 0; 12 when there was nothing to do; Info-ZIP's other codes for errors.

## Different in cash

- Windows has no zip command of its own; Explorer's "Compress to ZIP file" and
  `Compress-Archive` make archives without Unix modes.
- An archive records Unix modes from the Unix face (group bits the owner's), your
  account's id in the `ux` field, and times in the `UT` field; `-X` leaves the last two
  out. A drive and a leading `/` are left out of a member's name.
- Deflate is miniz_oxide's, so a compressed member's bytes, and sometimes its size, are
  not zip's own. Text is told from binary over the whole file.
- Files are compressed several at once, on every core, and written in order: the same
  archive as one at a time. A file over 64 MiB, an encrypted one and one `-l` converts
  are compressed as they are written.
- A name with characters beyond ASCII is stored as UTF-8 and marked so.
- `-sv` names the parts once the archive is written, not while it is, and `-sp` (a
  pause to change disks) is refused.
- `-U`, `-A`, `-J`, `-R`, `-DF`, the archive-bit options and the logs are refused.
