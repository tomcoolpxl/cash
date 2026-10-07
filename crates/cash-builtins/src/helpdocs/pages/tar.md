---
names: tar
see: gzip bzip2 xz zstd
---
## Description

`tar -cf ARCHIVE FILE...` packs files and folders into an archive, `tar -tf ARCHIVE`
lists it and `tar -xf ARCHIVE` unpacks it. `-z`, `-j`, `-J`, `--lzma`, `--lzip` and
`--zstd` compress the archive, or `-a` by its suffix; reading finds the compression by
itself. Compression runs as cash's `gzip`, `bzip2`, `xz` and `zstd` run by default:
the first three on every core, zstd on one. `-r` and `-u` add to an archive, `-A` joins archives, `--delete` removes
members and `-d` compares an archive with the files.

The options, messages, listings and exit status are GNU tar 1.35's, and so are the
archive's bytes for the same options. That takes in `-C`, `-T`, `--exclude` and its
kin, `--strip-components`, `--transform`, `--one-top-level`, `--wildcards`, `--owner`,
`--group`, `--mode`, `--mtime`, `--sort`, `--format=gnu|oldgnu|ustar|posix|v7`, `-v`,
`--totals`, `--quoting-style`, `TAR_OPTIONS` and `TAPE`.

It is all in Rust, in cash: no `tar.exe`, `gzip` or other program is started. What cash
writes, Windows' own `tar.exe` reads, and what `tar.exe` writes, cash reads.

## Windows notes

- Windows has its own `tar.exe` (bsdtar), with other options and messages. `tar` in
  cash is GNU's; `"$SYSTEMROOT/System32/tar.exe"` or `enable -n tar` reach Windows'.
- An absolute name loses its drive as GNU tar's loses its `/`: `C:/data/x` is stored as
  `data/x`, and tar says "Removing leading `C:/' from member names". `-P` keeps it.
- Windows has no Unix owners or modes. An archive records your account's name and id,
  and modes whose group bits are the owner's: `--mode=go-w` gives the usual 644.
  Extracting restores times and the read-only attribute (the owner's write bit), not
  owners or the other bits.
- A member whose name Windows cannot hold (`:`, `\`, a trailing dot or space, `CON`,
  `NUL` and the like) is refused with "Cannot open: Invalid argument"; the rest is
  extracted.
- A symbolic link is made where Windows allows one (Developer Mode, or an elevated
  shell); elsewhere it is refused as GNU tar refuses one, and the rest is extracted.
  Devices and FIFOs are refused.
- A sparse member, as GNU tar `-S` stores one, is read back whole. `-S` itself is
  accepted and stores the holes as zeros, as GNU does without it.
- `-Z`, `--lzop` and `-I PROGRAM` are refused: cash carries no such compressor. So are
  tapes, multi-volume archives, incremental snapshots (`-g`, `-G`), `--to-command`,
  `-W` and `-w`.
- `--sort=none` reads a folder in the order Windows gives, which on NTFS is by name.

## Examples

```
tar -czf site.tar.gz site/             # a gzip-compressed archive of a folder
tar -tvf site.tar.gz                   # list it, long form
tar -xf site.tar.gz -C /tmp/restore    # unpack it somewhere else
tar -xf release.tar.xz --strip-components=1
tar -caf logs.tar.zst --exclude='*.tmp' logs/
tar -xf pack.tar.gz --one-top-level    # everything under pack/
```
