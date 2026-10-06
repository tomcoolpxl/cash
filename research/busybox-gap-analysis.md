# BusyBox gap analysis

> **Discussion document, partly decided.** The decisions taken are listed below; the
> remaining sections are the analysis they were based on.

Already planned elsewhere and not re-proposed here: `dos2unix`/`unix2dos` (ROADMAP 7),
`fuser` and an `lsof` subset (ROADMAP 8), and an `ss` subset (ROADMAP 9).

## Decisions (2026-09-25)

The tier-1 questions were put to the user; the rest of this document is the evidence.

| Question | Decision |
|---|---|
| Small tier-1 tools | Adopt `pkill`, `pidof`, `killall`, `getopt`, `rev`, `clear`, `reset` |
| Larger tier-1 tools | Adopt `bc` only. `grep`/`egrep`/`fgrep`, `cmp`/`diff` and `gzip`/`gunzip`/`zcat` are **not** adopted; the "cash does not carry grep or diff" rule stands |
| Q2 kill-family matching | Case-insensitive, `.exe` optional, same rules as `pgrep`; access-denied and system processes skipped (reported with `-v`); `pkill` uses `kill`'s TERM escalation |
| Q4 `bc` scope | POSIX only (posixutils-rs import, hardened); no `dc` |
| Q9 `ping` | A Linux-flag `ping` builtin that shadows `ping.exe`, with pure iputils flags (`-t` is TTL, `-n` numeric); `ping.exe` stays reachable by path |
| Q10 doctor | Flag BusyBox shims only for tools whose BusyBox versions are known to break scripts (curated list) |

Q3, Q5–Q8 concern tools that were not adopted in this round and stay open.

## Decisions, second round (2026-10-06)

A second look at this machine, with Git for Windows' `usr/bin` off the Windows `PATH`
and BusyBox uninstalled, resolved about 90 candidate names under cash. The user chose to
build the tier-2 tools below (TODO.md phase 21, spec D74, ROADMAP item 19), plus four
names this analysis did not measure: `tput` (`tput setaf` 130k shell-script hits on
GitHub, `tput cols` 27k), `column` (`column -t` 27k), `xdg-open` (79k) and `pbcopy` (39k).

| Question | Decision |
|---|---|
| Tier 2 | Adopt `tput`, `stty`, `iconv`, `column`, `xxd`, `hexdump`, `uuidgen`, `free`, `watch`, `nice`/`renice`, `flock`, `nc`; add `xdg-open` (as `start`) and `pbcopy`/`pbpaste` |
| Q5 `flock` | One byte far past the end of the file, so readers of the lock file are not blocked; the lock lives on the shell's handle, for the command's lifetime or until the descriptor closes; directories refused; `flock FD` through cash's table only |
| Q6 `nice` | `-20…-11` HIGH, `-10…-1` ABOVE_NORMAL, `0` NORMAL, `1…10` BELOW_NORMAL, `11…19` IDLE; never REALTIME; bare `nice` maps the class back |
| Q7 `free` | `Swap:` is the page file; no `Commit:` row, since scripts parse `free` by `Mem:` and `Swap:` |
| Q8 `nc` | OpenBSD's flags; `-e`/`-c` refused |
| Q3 compressors | Still not adopted; to be discussed next. On this machine `gunzip` and `zcat` resolve to nothing even with Scoop's gzip installed, and `tar.exe` reads only archives |
| `grep`, `diff`, `cmp`, `patch`, `strings` | Still not adopted |

## Method

1. **BusyBox applet set.** The union of three lists, 390 names in total:
   - the classic applet reference at <https://busybox.net/downloads/BusyBox.html>
     (310 names, including many that were later removed);
   - BusyBox 1.38.0 as packaged by Debian testing, from the `busybox(1)` manual's
     "currently defined functions" (274 names);
   - **busybox-w32** `v1.38.0-FRP-6075` (Ron Yorston's native Windows port), from
     `busybox --list` on the Scoop install on this machine (178 names). This list shows
     what someone else has already found implementable on Win32, and it includes
     Windows-only applets (`drop`, `cdrop`, `pdrop`, `jn`, `uuidgen`, `whois`, `make`).
2. **cash inventory.** `target/debug/cash.exe --norc -c 'enable -a'` lists 169 enabled
   builtin names: Bash builtins, cash's own tools (`crates/cash-builtins/src/factory.rs`),
   the uutils set (`crates/cash-coreutils-builtins`, feature `coreutils.all`), and the
   process-backed `awk`/`sed` (`crates/cash-shell/src/bundled.rs`). `cash doctor` reports
   "167 commands answered by cash itself". The two-name difference comes from doctor's
   own counting and was not investigated; it does not affect this analysis.
3. **Windows and Git for Windows collisions.** I checked each name against
   `C:\Windows\System32` (`.exe`/`.com`, including `OpenSSH\`),
   `C:\Program Files\Git\usr\bin` and `mingw64\bin`, and `type -p` under cash on this
   machine. On this machine many of the missing names (`pkill`, `pidof`, `killall`,
   `free`, `watch`, `strings`, `hexdump`, `nc`, `wget`, `flock`, `bc`, `dc`, `vi`,
   `gunzip`, `zcat`, `egrep`) resolve to **Scoop shims for `busybox.exe`**. That is the
   masquerade D35 describes, and doctor does not flag it because only `grep` and `diff`
   are in its `EXPECTED` table.
4. **Usage evidence.** GitHub code-search hit counts (`language:Shell`, exact phrase,
   queried 2026-09-25 through `gh api search/code`) for a typical invocation of each
   tool. These numbers are coarse: code search tokenizes punctuation loosely, counts
   files rather than invocations, and rounds large totals. Use them to rank tools against
   each other, not as absolute measures. Baselines for calibration: `| grep` 4.9M,
   `| sed` 3.9M, `| sort` 1.1M.
5. **Implementation sources.** I checked the crates and projects that could back each
   candidate: uutils `grep` (`uu_grep` 0.2.0, MIT, the build Microsoft Coreutils ships),
   uutils `diffutils` (MIT/Apache-2.0, `diff` and `cmp`), uutils `procps` (MIT, still
   procfs-bound), and the local `posixutils-rs-reference` checkout (MIT: `bc`, `ed`,
   `cmp`, `diff`, `patch`, `strings`, `cal`, `compress`, `uuencode`, `logger`, `nice`,
   `renice`, `crontab`).

## Inventory summary

| Bucket | Count |
|---|---:|
| BusyBox applet names (union of the three lists) | 390 |
| Already answered by cash | 94 |
| Missing from cash | 296 |
| … provided by the shell itself (`sh`, `bash`, `ash`, `hush`, `lash`, `[[`, `time`) | 7 |
| … already planned (`dos2unix`, `unix2dos`, `fuser`) | 3 |
| … **candidates, tier 1** | 17 |
| … **candidates, tier 2** | 24 |
| … **candidates, tier 3** | 41 |
| … covered elsewhere | 22 |
| … not applicable | 182 |

cash also answers 75 names that BusyBox lacks, mostly the rest of coreutils (`b2sum`,
`basenc`, `csplit`, `numfmt`, `ptx`, `tsort`, …) and cash's own `pstree`, `tree`,
`winpath`, `elevate`, `start`, `detach` and `coolfetch`.

The tier-1 count covers 9 decisions, because several names share one implementation
(`grep`/`egrep`/`fgrep`; `gzip`/`gunzip`/`zcat`; `pkill`/`pidof`/`killall`; `clear`/`reset`).

**Windows executables cash already shadows**, for context: `sort.exe`, `find.exe`,
`more.com`, `tree.com`, `timeout.exe`, `expand.exe`, `hostname.exe` and `whoami.exe` are all
reached only after cash's builtin of the same name. `type -a` lists both, and `enable -n`
gives access to the Windows tool. That is the established "honest shadowing" pattern. The
bar for a new shadow is that the Windows tool must not be what a bash script meant by the
name.

## Candidates

Sizes: **S**, a day or two including tests; **M**, about a week or an import with
porting; **L**, multi-week or a large option surface.

The "agrees with cash" column applies README's inclusion test: does the tool have to
agree with cash about something cash owns (PIDs, paths, console modes, file descriptors,
CRLF, command resolution)? A yes strengthens the case beyond "it is missing".

### Tier 1: high value, low or bounded effort

| Tool(s) | Script usage (GitHub hits) | Backing | Size | Agrees with cash? | Shadowing risk | Supported / refused |
|---|---|---|:-:|---|---|---|
| `pkill`, `pidof`, `killall` | `pkill` 282k, `killall` 186k, `pidof` 81k | Existing `pgrep` matcher (`cash-builtins/src/pgrep.rs`), the Toolhelp snapshot in `cash-win32/src/process.rs`, and `kill`'s D21/D22 signal path. uutils `procps` is procfs-only and unusable. | S | **Yes**: PIDs (D22), and the same signal semantics as `kill` | None on stock Windows (`taskkill` has a different name). Git for Windows ships none. On this machine all three are BusyBox shims. | `pkill`: pgrep's `-P -x -v` plus `-SIG`/`--signal`, `-e` (echo), `-c` (count), `-n`/`-o` (newest/oldest). `pidof`: `-s`, `-x` (no-op; scripts are processes too), `-o PID`. `killall`: `-SIG`/`-s`, `-e`, `-I`, `-q`, `-v`, `-w`, `-r`. **Refused:** `-f`/`--full` (needs another process's command line from its PEB, refused on the same grounds as `ps -o`), `-u`/`-U`/`-g`/`-G`/`-t` and `killall -i` in the first cut, `-Z`/`--ns` (Linux). |
| `grep`, `egrep`, `fgrep` | `\| grep` 4.9M, the most used external command | `uu_grep` 0.2.0 (MIT), the same crate Microsoft Coreutils ships. It depends on Oniguruma through `onig_sys` (C, BSD-2), so it adds a `cc` build step. | M | Partly: `grep -r`/`-l` builds paths, so it may need D48's path-rendering allowlist | **High for Git users**: Git's GNU `grep.exe` is on nearly every PATH. The same shadow occurs today with Microsoft Coreutils, which is `uu_grep`. | Whatever `uu_grep` implements, provided unsupported flags are rejected rather than ignored (verify first). `egrep`/`fgrep` map to `-E`/`-F`, with GNU 3.8's obsolescence warning optional. |
| `cmp` | `cmp -s` 159k | uutils `diffutils` (`src/cmp.rs`). Reference: posixutils-rs `file/cmp.rs`. | S | No | Git's GNU `cmp.exe`. `comp.exe`/`fc.exe` have different names. | `-s`/`--quiet`, `-l`, `-b`, `-i SKIP`, `-n LIMIT`, `--help`. Nothing to refuse. |
| `diff` | `diff -u` 90k, the standard test-script comparator | uutils `diffutils` (`diff.rs`, `unified_diff.rs`, `context_diff.rs`, `ed_diff.rs`, `side_diff.rs`). Reference: posixutils-rs `text/diff.rs`. | M | Weakly: `diff -r` echoes the paths it is given; output must honour D20 when CRLF is involved | **High**: Git's GNU diffutils. README and doctor currently say cash "does not carry grep or diff". | `-u`/`-U N`, `-c`/`-C N`, `-q`, `-s`, `-r`, `-N`, `-a`, `-i`, `-w`, `-b`, `-B`, `--label`, `-e`, `-y`, `--color`, as far as uutils supports them. **Refused** until implemented: `--strip-trailing-cr` (or it becomes a D20 question), `-D`/`--ifdef`, `--line-format`, `-x`/`-X` exclude patterns. |
| `gzip`, `gunzip`, `zcat` | `gunzip` 105k, `zcat` 65k, `gzip -d` 33k | `flate2` with the pure-Rust `miniz_oxide` backend (MIT/Apache-2.0). No uutils implementation. | S–M | No | Git ships GNU `gzip.exe`, but its `gunzip`/`zcat` are extensionless `sh` scripts. Stock Windows has no gzip (`tar.exe` only handles `.tar.gz`). On this machine `gunzip`/`zcat` are BusyBox shims. | `-c -d -f -k -l -n -N -q -r -t -v -S SUF -1…-9 --fast --best`, stdin to stdout, multi-member input, `zcat -f` passthrough. **Refused:** `--rsyncable`, `-Z`/LZW output. `uncompress` of `.Z` input stays in tier 3. |
| `bc` | `\| bc` 206k, the standard float arithmetic in shell scripts | posixutils-rs `calc/bc.rs` + `bc_util` (MIT), imported once the way `awk` was. | M | No | **None**: stock Windows and Git for Windows ship no `bc`. On this machine it is a BusyBox shim. | POSIX bc, `-l` (math library), `-q`, `-s` (strict). GNU extensions scripts use (`print`, `read()`, `else`, `&&`/`\|\|`, `last`, `-w`) are an open question (Q4). **Refused:** anything not implemented, with an error naming the extension. |
| `getopt` (util-linux) | `getopt -o` 29k | New code, pure logic. | S | **Yes**: its output is shell-quoted text for `eval set -- "$(getopt …)"`, so it must quote for cash's parser | Git ships util-linux `getopt.exe` (MSYS). Stock Windows has none. | `-o`, `-l`/`--longoptions`, `-n`, `-q`, `-Q`, `-a`, `-u`, `-s sh\|bash`, `-T` (exit 4 means "enhanced getopt"). **Refused:** `-s csh\|tcsh`. |
| `rev` | `\| rev` 68k; the `rev \| cut -d. -f1 \| rev` idiom | New code, trivial. | S | **Yes**: a trailing `\r` must stay at the line end (D20), not become a leading `\r` | None (Git lacks `rev`). | All of util-linux's surface: files or stdin, `-0`/`--zero`. |
| `clear`, `reset` | Common in interactive menus and installers | VT sequences (`ESC[H ESC[2J ESC[3J`), since ConPTY and Windows Terminal are the target; `reset` also restores the console modes cash owns. | S | **Yes**: console modes | Git's ncurses `clear.exe`/`reset.exe` depend on `TERM`/terminfo. `cls` is a cmd builtin, not an executable. (Correction, found while building it: `reset` does shadow `C:\Windows\System32\reset.exe`, the Remote Desktop `reset session` command; see spec D55.) | `clear [-x]` (`-x` keeps scrollback). `reset`: `-q`/`-e`/`-k` from tset. **Refused:** `-T TERM` beyond `xterm*`/`vt*`. |

### Tier 2: clear value, more effort or more collision risk

| Tool(s) | Script usage (GitHub hits) | Backing | Size | Agrees with cash? | Shadowing risk | Supported / refused |
|---|---|---|:-:|---|---|---|
| `xxd` | `xxd -p` 13k | New code. | S | No | Git ships vim's `xxd.exe`: aim for byte-identical output. | `-p`, `-r`, `-r -p`, `-i`, `-l`, `-s`, `-c`, `-g`, `-u`, `-b`, `-e`, `-o`, `-C`, `-n`. |
| `hexdump`, `hd` | `hexdump` 28k | New code. `od` already exists. | S | No | None (Git lacks both). BusyBox shims on this machine. | `-C`, `-v`, `-n`, `-s`, `-x`/`-d`/`-o`/`-c`/`-b`. **Refused** (or phase 2): `-e FORMAT` format strings. |
| `strings` | `\| strings` 6k, `strings -a` 2k | posixutils-rs `dev/strings.rs` as reference, or new code. | S | No | **Sysinternals `strings.exe`** has the same name and different flags (`-n` matches, `-u`/`-s` differ). Its absence from `type -a` output needs doctor coverage. | `-a`, `-n N`/`-N`, `-t d\|o\|x`, `-e s\|S\|l\|b\|L\|B`. UTF-16LE (`-el`) is especially useful on Windows. |
| `iconv` | `iconv -f` 14k | `encoding_rs` (MIT/Apache-2.0) for decoding, plus hand-written UTF-16/32 encoders; or Win32 `MultiByteToWideChar`/`WideCharToMultiByte` for every installed code page. | M | Partly: D41 (BOM) and the UTF-8 console | Git's GNU `iconv.exe`. | `-f`, `-t`, `-l`, `-c`, `-o`, `//IGNORE`. **Refused:** `//TRANSLIT` (no honest table without a GNU-sized dataset). High Windows value: `iconv -f UTF-16LE -t UTF-8` for PowerShell, `reg export` and `wmic` output. |
| `watch` | `watch -n` 12k | New code: alternate screen and re-running through cash (GNU runs `sh -c`, which is cash anyway). | M | Yes: runs commands through cash's resolution | None. | `-n`, `-t`, `-d`, `-e`, `-g`, `-x`, `-c`, `-p`. **Refused:** `-b` (beep) is optional; `-q`/`-r` (procps-ng 4.x) where not implemented. |
| `free` | `free -m` 34k | `cash_win32::sysinfo::memory_status()` already exists (`GlobalMemoryStatusEx`); `top` uses it. | S | Should match `top`'s memory line | None. | `-b -k -m -g -h --si -t -w -s N -c N`. Swap row: see Q7. `buff/cache` has no direct equivalent: print standby list as cache (PDH) or `0` and document it. |
| `nc` | `nc -z` 64k (wait-for-port loops in CI) | `std::net` + `socket2` (MIT/Apache-2.0). | M | No | No Windows `nc`. nmap ships `ncat` (a different name). Scoop `netcat`/BusyBox shims. | `-z`, `-w`, `-v`, `-n`, `-l`, `-p`, `-u`, `-k`, `-q`, `-N`, `-4`/`-6`. **Refused:** `-e`/`-c` (exec), see Q8. `-x` proxy and `-U` unix sockets in phase 2 (AF_UNIX exists since Windows 10 1803). |
| `unzip` | `unzip -q` 83k | `zip` crate (MIT). | M | No | Git's Info-ZIP `unzip.exe`. Windows `tar.exe` also extracts zip (`tar -xf a.zip`). | `-l`, `-o`, `-n`, `-q`, `-d DIR`, `-p`, `-j`, `-t`, `-x`, `-Z1`. **Refused:** `-a` text conversion, and `-P` for encryption the crate cannot read. |
| `xz`, `unxz`, `xzcat`, `lzma`, `unlzma`, `lzcat` | `xz -d` 10k | `liblzma` crate (C liblzma, 0BSD) or pure-Rust `lzma-rs` (MIT, slower, mostly a decoder). | M | No | Git's mingw64 `xz.exe`. `tar.exe` covers `.tar.xz`. | `-d -c -k -f -z -t -l -q -v -0…-9 -e -T`. **Refused:** `--format=raw` filter chains in the first cut. |
| `bzip2`, `bunzip2`, `bzcat` | `bzip2` 92k (inflated by `.tar.bz2` mentions) | `bzip2` crate with the pure-Rust `libbz2-rs-sys` backend. | S–M | No | Git's `bzip2.exe`. `tar.exe` covers `.tar.bz2`. | `-d -z -c -k -f -t -q -v -s -1…-9`. |
| `stty` | `stty -echo` 15k; `stty size` is also common | `GetConsoleMode`/`SetConsoleMode`, `GetConsoleScreenBufferInfo`. | M | **Yes**: cash owns console modes (read editor, `read -s`, ConPTY) | Git's MSYS `stty.exe` acts on MSYS ptys and does nothing useful on a native console. | `size`, `-g` and restoring from its output, `echo`/`-echo`, `icanon`/`-icanon`, `raw`/`-raw`, `sane`, `cols`/`rows` queries, `-a` (a reduced listing). **Refused:** baud rates, parity, `intr`/`erase`-style control-character assignment, `-F DEVICE`. |
| `nice`, `renice` | `nice -n` 21k, `renice` 7k | `SetPriorityClass`, and `CreateProcess` priority flags for `nice CMD`. posixutils-rs `process/nice.rs`/`renice.rs` as references. | S | Yes: PIDs, and `ps -efj`'s priority column | Git's MSYS `nice.exe`. | `nice [-n N] CMD`, bare `nice` (prints the niceness), `renice [-n] N -p PID…`. Mapping and REALTIME: see Q6. **Refused:** `renice -g`/`-u` (no process groups or user scoping). |
| `flock` | `flock` 96k | `LockFileEx`/`UnlockFileEx`. `flock FD` goes through cash's own fd table (D26), which a builtin can reach but an external tool cannot. | M | **Yes**: file descriptors above 2 (D26) | None. BusyBox shim on this machine. | `-s`, `-x`, `-n`, `-w SECS`, `-E CODE`, `-o`, `-u`, `-c CMD`, `FILE CMD…`, `FD`. Semantics: see Q5. |
| `uuidgen` | 42k | `UuidCreate`/`UuidCreateSequential`, or the `uuid` crate (MIT/Apache-2.0) for v3/v5. | S | No | None (it is a busybox-w32 applet). | `-r`, `-t`, `--md5`/`--sha1` with `-n NS -N NAME`, `-x` (hex). |
| `patch` | `patch -p1` 70k | posixutils-rs `text/patch.rs` (reference or import), or `diffy` (MIT/Apache-2.0) as a base. | M–L | CRLF handling (D20) | Git's GNU `patch.exe`. Git users also have `git apply`. | `-p N`, `-i`, `-R`, `-N`, `--dry-run`, `-b`, `-o`, `-s`, `-f`, `-F`, `-d`, `-E`. **Refused:** git binary patches, `-e` ed scripts, and reject files beyond `.rej`. |
| `ts` (moreutils) | `\| ts` 10k | New code. | S | No | None. | `ts [FORMAT]`, `-i`, `-s`, `-m`. **Refused:** `-r` (relative-time rewriting of existing timestamps) in the first cut. |

### Tier 3: implementable but low value, or needing a design first

| Tool(s) | Why tier 3 | Size | Notes |
|---|---|:-:|---|
| `ping`, `ping6` | `ping -c` 95k is real usage, but `ping.exe` has the same name and **different flags**: `-n` is count and `-c` is "routing compartment". Unelevated, `ping -c 1 127.0.0.1` prints "Access denied. Option -c requires administrative privileges." and exits 1 (verified). Elevated, it runs Windows' default four echoes. | M | `IcmpSendEcho2`/`Icmp6SendEcho2`, no raw sockets. Shadowing `ping.exe` is the decision; see Q9. |
| `wget` | `wget -q` 221k, but Windows ships real `curl.exe`. The option surface is large and TLS is a dependency choice (rustls or schannel). | L | If ever done: `-q`, `-O FILE\|-`, `-c`, `-T`, `-t`, `--header`, `-U`, `--no-check-certificate`, `-P`. Recursive mode refused. |
| `setsid` | 56k, but cash's `detach` and `nohup` already cover it. | S | Could be an alias for `detach` with `-f`/`-w`. `-c` (controlling terminal) refused. |
| `ionice` | 8k. A process can only set its own I/O priority through a documented API (`PROCESS_MODE_BACKGROUND_BEGIN`). | S | `ionice -c 3 CMD` is honest via background mode. Other PIDs and classes 1–2 refused. |
| `taskset` | Useful, but rare in scripts. | S | `SetProcessAffinityMask`, `-c LIST`, `-p`. More than 64 CPUs needs processor groups; refuse there. |
| `run-parts` | 9k. Scripts rarely use it on Windows. | S | **Agrees with cash**: "executable" must mean D8 resolution (PATHEXT), not a mode bit. `--test`, `--list`, `--regex`, `-a`, `--exit-on-error`. |
| `script`, `scriptreplay` | 6.5k. | M–L | A ConPTY recorder is plausible (cash owns ConPTY), but timing files and `-c` add surface. |
| `mountpoint` | `mountpoint -q` usage is low. | S | `GetVolumePathNameW`: a drive root or mounted-folder junction is a mount point. `-d` (device number) refused. |
| `logger` | Moderate on Linux, low on Windows. | S | `ReportEventW` into the Application log. An unregistered source works but Event Viewer shows a "description not found" warning, and registering a source needs admin. Decide first. |
| `mkpasswd`, `cryptpw` | Rare (cloud-init hashes). | S | `sha-crypt`/`md5-crypt` crates (RustCrypto, MIT/Apache-2.0). **Shadowing trap:** Git's `mkpasswd.exe` is Cygwin's `/etc/passwd` generator, a completely different tool. |
| `chown`, `chgrp` | Appear in install scripts. uutils excludes them on Windows. | M | Setting an owner needs `SeRestorePrivilege`/`SeTakeOwnershipPrivilege`, and there are no POSIX groups. Choose between a D34-style warn-and-return-0 and refusal. |
| `ed` | Some scripts use `ed -s file <<<`. | M | posixutils-rs `editors/ed` import. |
| `dc` | 5k. | M | Would share a numeric core with `bc`. |
| `cpio` | 68k (dominated by initramfs scripts, which are Linux-only). | M | `tar.exe` (bsdtar) reads and writes cpio: `tar --format=cpio`. |
| `uncompress` | Low. | S | posixutils-rs `xform/compress.rs`. |
| `lzop`, `lzopcat`, `unlzop`, `lzmacat` | Rare. | S–M | Aliases for the xz family, or omit. |
| `uuencode`, `uudecode` | Rare. | S | posixutils-rs `xform/uu*.rs`. |
| `usleep` | Obsolete; `sleep 0.1` works. | S | Trivial alias. |
| `cal` | Interactive use only. | S | posixutils-rs `datetime/cal.rs`. |
| `ascii`, `crc32`, `sha3sum` | Low. `sha3sum` may come from uutils' hashsum family. | S | Check uutils before writing any code. |
| `fsync`, `fallocate` | Low. `sync` and `truncate -s` exist. | S | `FlushFileBuffers`; `SetFileInformationByHandle(FileAllocationInfo)`. |
| `w` | Low. | S | Composes `who` and `uptime`, which cash already has. |
| `ttysize` | Low. | S | Would share `stty size`'s code. |
| `inotifyd` | Scripts use `inotifywait` (not BusyBox) more. | M | `ReadDirectoryChangesW`. If watching is wanted, design `inotifywait` instead. |
| `dnsdomainname` | Low. | S | `GetComputerNameExW(ComputerNameDnsDomain)`. |
| `ipcalc` | Low. | S | Pure logic. |
| `drop`, `cdrop`, `pdrop` | busybox-w32 applets that run a command **without** elevation. They are the inverse of `elevate`. | S–M | `CreateRestrictedToken` or `SaferComputeTokenFromLevel`. Interacts with D42. |
| `pipe_progress` | Rare. | S | Trivial. |

## Covered elsewhere

| Applet | Covered by | Notes |
|---|---|---|
| `tar` | `C:\Windows\System32\tar.exe` | bsdtar (libarchive). It handles gz, bz2, xz, zip and cpio, and cash does not shadow it. |
| `nslookup` | `nslookup.exe` | Same name, broadly the same interactive and one-shot usage. |
| `netstat` | `netstat.exe` | Different flags (`-ano`). ROADMAP 9's `ss` is the Linux-shaped answer, and `netstat.exe` stays unshadowed. |
| `traceroute`, `traceroute6` | `tracert.exe` | Different name, so no shadow question arises. |
| `telnet`, `tftp` | `telnet.exe`/`tftp.exe` (optional Windows features) | |
| `ftpget`, `ftpput` | `curl.exe` (`curl -T`, `curl ftp://…`) | `ftp.exe` exists but is interactive only. |
| `whois` | Sysinternals `whois.exe`, PATH tools | |
| `vi` | `$EDITOR`: vim (Git ships it), nano, notepad | An editor is personal choice, not script plumbing. |
| `man` | `--help`; no man pages on Windows | |
| `make`, `pdpmake`, `mim` | GNU make from winget or Scoop | `mim` is BusyBox 1.38's Makefile-style script runner. |
| `ar` | Toolchains (`llvm-ar`, binutils) | |
| `su` | cash `elevate` (D45), Windows 11 `sudo.exe`, gsudo | |
| `jn` (busybox-w32 junction) | cash `ln -s` on a directory (D27) | |
| `chattr`, `lsattr`, `fatattr` | `attrib.exe` | Windows file attributes are not ext2 attributes. |
| `httpd` | `python -m http.server`, `npx serve`, PATH tools | |

## Not applicable

Grouped by the reason each one has no Windows meaning.

- **Provided by cash itself (7):** `sh`, `bash`, `ash`, `hush`, `lash` (cash resolves
  shell names to itself), `[[` and `time` (shell keywords).
- **Linux kernel, modules and namespaces (34):** `depmod insmod rmmod lsmod modprobe
  modinfo devmem readprofile dmesg sysctl klogd syslogd logread adjtimex hwclock rtcwake
  uevent mdev i2cdetect i2cdump i2cget i2cset i2ctransfer watchdog setarch linux32 linux64
  unshare nsenter setpriv chrt chroot pivot_root switch_root`.
- **Init, runlevels, service supervision and cron (28):** `init linuxrc run-init nuke
  halt poweroff reboot runlevel getty sulogin cttyhack acpid resume killall5
  start-stop-daemon runsv runsvdir sv svc svok svlogd chpst envdir envuidgid setuidgid
  softlimit crond crontab`. Windows services (`sc.exe`) and Task Scheduler (`schtasks.exe`)
  are different models. A `crontab` that translated to `schtasks` would present cron
  semantics it cannot deliver (missed-run behaviour, environment, `MAILTO`), which fakes
  rather than refuses.
- **Storage, filesystems, block devices and mounts (35):** `mount umount losetup
  nbd-client fdisk partprobe mkdosfs mkfs.minix mkfs.vfat mke2fs fsck fsck.minix mkswap
  swapon swapoff blkid findfs blockdev blkdiscard fstrim fsfreeze hdparm lsscsi
  raidautorun freeramdisk fdflush fdformat rdev flash_lock flash_unlock ubirename volname
  eject mt getfattr`.
- **Linux virtual console and framebuffer (18):** `openvt chvt deallocvt kbd_mode loadfont
  setfont loadkmap dumpkmap setkeycodes showkey setconsole setlogcons fbset fbsplash
  resize mesg vlock beep`.
- **Network configuration (26):** `ifconfig ip ipaddr iplink iproute iprule iptunnel route
  arp arping nameif ifup ifdown ifenslave ifplugd brctl vconfig tunctl tc slattach zcip
  udhcpc udhcpc6 udhcpd dhcprelay dumpleases`. `ipconfig`, `route.exe`, `arp.exe` and
  `netsh` are Windows' own. `route`/`arp` exist with the same names but different flags,
  and as configuration tools they must not be shadowed.
- **Network daemons, mail, printing and serial (21):** `inetd tcpsvd udpsvd fakeidentd dnsd
  ftpd tftpd telnetd lpd lpq lpr sendmail popmaildir makemime reformime rdate pscan
  ssl_client microcom rx chat`.
- **Accounts and login (9):** `login passwd adduser addgroup deluser delgroup chpasswd
  nologin last`. There is no wtmp. The Security log needs admin and is not the same
  record.
- **POSIX-only kernel objects (4):** `mkfifo mknod ipcs ipcrm`. Named pipes live in
  `\\.\pipe\`, not the filesystem, and there is no SysV IPC.
- **Linux package formats (4):** `dpkg dpkg-deb rpm rpm2cpio`.
- **Implementable but not worth carrying (3):** `length` (`${#v}`), `catv` (`cat -v`),
  `nmeter`.

## Open questions for the top candidates

### Q1. Reopen "cash does not carry grep or diff"?

README and doctor's `EXPECTED` table say `grep`/`diff` are left to PATH because "they do
not have to agree with the shell". Two facts have changed that argument. `uu_grep` now
exists and is exactly what Microsoft Coreutils ships, so D48's "the builtin *is* the
recommended install" argument applies to it as it did to coreutils. And uutils
`diffutils` exists under MIT/Apache-2.0.

- **(a) Keep the rule.** No change. Scripts on a bare Windows box fail on `grep`, but the
  failure is loud and doctor names it.
- **(b) Bundle `grep` only.** The biggest single gap closes, for about +1–2 MB and a C
  dependency (Oniguruma via `cc`). GNU grep users get uutils behaviour until they run
  `enable -n grep`. Doctor must stop listing `grep` as expected and start reporting the
  shadow.
- **(c) Bundle `grep`, `cmp` and `diff`.** As (b), and test suites using `diff -u` or
  `cmp -s` work out of the box. `diff` output is compared byte-for-byte by tests, so it
  needs a differential gate against GNU diffutils like sed's (ROADMAP 3).
- **Sub-question:** should `grep -r` output go through D48's path-rendering allowlist?
  `uu_grep` joins recursive paths itself, so it *builds* paths, which is D48's test.

### Q2. Process-kill family: matching rules

- **Case:** Windows image names are case-insensitive. Should `pidof`/`killall`/`pkill`
  match case-insensitively by default (as D16 does for globbing), or follow Linux and
  require `-i`/`-I`?
- **`.exe` suffix:** should `killall notepad` match `notepad.exe`? The options are
  "suffix optional", "exact name only" or "either". `pgrep` already makes a choice here,
  and the family should follow it for consistency.
- **Scope:** `killall NAME` reaches every process the user may open, not just cash's jobs.
  Should access-denied targets be silently skipped (Linux prints "Operation not
  permitted") or reported? And should `killall` refuse `svchost`/`csrss`-class system
  images outright?
- **Signal mapping:** `pkill` defaults to `TERM`. It should reuse D21's asynchronous
  escalation exactly as `kill` does, and I recommend doing so. Say so explicitly in the spec.

### Q3. Compressors: which formats, which backends

- **(a) gzip family only** (pure Rust, small). xz and bzip2 stay with `tar.exe` and PATH.
- **(b) gzip + bzip2 + xz.** Pure Rust for bzip2 is available, but for xz the choice is
  between the C `liblzma` (fast, full-featured, a C build) and `lzma-rs` (pure Rust,
  mostly decode, slower).
- **(c) Add `unzip`.** It shadows Git's Info-ZIP, which scripts rarely depend on in
  detail, and `tar -xf` already works for zip.
- **Consequence for all three:** each shadows a Git for Windows binary, so doctor and
  `type -a` must report it, as planned for `dos2unix`. Decide whether the "honest
  shadow" rule needs a doctor line per name or one summary line.

### Q4. `bc`: POSIX or GNU surface?

The posixutils-rs import is POSIX bc. Real scripts often use GNU extensions: `print`,
`read()`, `else`, `&&`/`||`, `++`, `-l` functions beyond POSIX (none are GNU-only, but
`scale` behaviour differs subtly).

- **(a) POSIX only,** with explicit errors on the extensions. This is honest, but some
  scripts break loudly.
- **(b) POSIX plus the common GNU extensions,** with a corpus gate against GNU bc as for
  awk and sed. More work, far fewer surprises.
- **Also decide:** whether to add `dc` at the same time, since GNU bc and dc share a
  numeric core.

### Q5. `flock`: advisory semantics on a mandatory-lock OS

`LockFileEx` locks are mandatory byte-range locks bound to a handle and a process.

- **Lock range:** lock a byte range far beyond EOF (for example offset `2^63-1`), so
  other readers and writers of the lock file are not blocked, or lock byte 0 and accept
  that `cat lockfile` from another process fails.
- **Children:** Linux `flock FILE cmd` holds the lock on an open file description that
  `cmd` inherits. On Windows the lock belongs to cash's handle and process. Keeping the
  lock for `cmd`'s lifetime is natural, but a `cmd` that re-execs detached will not hold
  it. Document this, or refuse the pattern?
- **Directories:** `flock /var/lock/dir cmd` is legal on Linux. `LockFileEx` on a
  directory handle is not. The choices are to refuse, or to lock a hidden sidecar file,
  which invents state.
- **`flock FD`:** only works for fds in cash's table (D26). Say so, and fail loudly for an
  fd cash does not own.

### Q6. `nice`/`renice`: value mapping

- There are six Windows priority classes for 40 nice values. Proposed mapping: `-20…-11`
  → HIGH, `-10…-1` → ABOVE_NORMAL, `0` → NORMAL, `1…9` → BELOW_NORMAL, `10…19` → IDLE.
- **REALTIME:** never set it (it can starve the system), or set it only with an explicit
  flag?
- **Printing:** bare `nice` and `ps` should report the inverse mapping. Is a lossy
  round-trip acceptable, or should cash remember the requested value for processes it
  started, like `umask`?

### Q7. `free`: the Swap row

- **(a)** Report page-file usage (`GetPerformanceInfo` commit minus physical memory, or
  per-pagefile PDH counters) as "Swap". This is familiar, but it approximates.
- **(b)** Report "Commit" (limit/used), matching `top`, and print Swap as zeros with a
  documented divergence.
- **(c)** Print both rows. Scripts parsing `free | awk '/Mem:/{…}'` keep working either
  way. Scripts parsing `/Swap:/` need (a) or (c).

### Q8. `nc`: include `-e`/`-c`?

- **(a) Refuse** `-e`/`-c`, as OpenBSD nc does. This is the safer default, and the
  wait-for-port use (`nc -z`) is what most scripts want.
- **(b) Implement** them through cash's command host, as BusyBox and ncat do. This is
  convenient for debugging but is a remote-shell primitive that security tools flag.

### Q9. Own `ping`, shadowing `ping.exe`?

`ping -c 1 host && …` is common in scripts, and Windows `ping.exe` reads `-c` as a
routing compartment. Unelevated, it prints "Access denied. Option -c requires
administrative privileges." and exits 1 (verified on this machine), so
`ping -c 1 host || fail` reports every host as down. Elevated, it sends four echoes in
compartment 1 instead of one.

- **(a) Don't carry `ping`.** Document the trap and have doctor flag `ping.exe` as a
  "same name, different flags" collision, the way it flags DOS `find`/`sort`.
- **(b) Carry a Linux-flag `ping`** on `IcmpSendEcho2`, shadowing `ping.exe` with
  `enable -n ping` as the escape. This is consistent with `find`/`sort`/`timeout`, but
  users who type Windows flags (`-n 3`, `-t`) at the cash prompt get errors. Accepting
  `-n` as count too would be ambiguous, because Linux `-n` means numeric output.
- **(c) Carry it, but reject Windows-only flags with a hint** giving the Linux spelling,
  in the style of `ss -ano`.

### Q10. Doctor coverage of BusyBox shims

On this machine, 16 of the missing names (13 of them tier-1 or tier-2 candidates) resolve to Scoop BusyBox shims that
doctor never mentions, because its BusyBox check only runs for names in `EXPECTED`. The
question is independent of what gets built: extend the check to every resolved name
that is not a cash builtin, or keep doctor focused on the few tools whose BusyBox
versions actually break scripts (`awk`, `sed`, `grep`, `find`, `xargs`)?
