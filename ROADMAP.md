# Cash roadmap

This is the central ordering document for planned compatibility and bundled-userland
work. Detailed investigations and implementation checklists remain in `research/`, but
their priority is set here.

Urgent regressions, crashes, security problems, and failing CI may interrupt this order.
Feature work follows the sequence below.

## Current sequence

| Order | Workstream | Status | Detailed source |
| ---: | --- | --- | --- |
| 1 | Finish the advertised Bash 5.2 interface | **Complete** | [Bash 5.2 remaining plan](research/bash-reference/bash-5.2-remaining-plan.md) and [gap audit](research/bash-reference/bash-5.2-gaps.md) |
| 2 | Absorb and port a native `awk` | **Complete** | [`posixutils-rs` AWK evaluation](research/posixutils-rs-evaluation.md) |
| 3 | Absorb and integrate a native `sed` | **Complete** | [`uutils/sed` evaluation](research/uutils-sed-evaluation.md) |
| 4 | Expand bundled userland | **Complete** | `stat`, `tty`, `install`, `pathchk`, `nohup`, `who`, `users`, `pinky`, `logname`, `hostid` |
| 5 | Bash 5.3 compatibility work | **Complete** (cash reports `BASH_VERSION=5.3.15`) | [Bash 5.3 audit](research/bash-reference/bash-5.3-audit.md): every item probed against Bash 5.3.15, by script and on a ConPTY |
| 6 | Differential corpus fixes | **Active** | [`tests/corpus`](tests/corpus): sourced sed/awk/Bash one-liners run against Git Bash 5.3 with GNU sed and gawk |
| 7 | Line-ending controls: sed `-b`, `CASH_EOL`, `dos2unix`/`unix2dos` | **Complete** | Section 7 below; spec D49 |
| 8 | `fuser` and an `lsof` subset | **Complete** | Section 8 below; spec D50 |
| 9 | `ss` subset | **Complete** | [`ss` evaluation and decisions](research/ss-evaluation.md); spec D51 |
| 10 | BusyBox-gap tools | **Complete** (`pkill`/`pidof`/`killall`; `getopt`, `rev`, `clear`, `reset`; `bc`; `ping`; doctor's BusyBox check) | [BusyBox gap analysis and decisions](research/busybox-gap-analysis.md); spec D54 |
| 11 | Graceful Ctrl-C and `TERM` for cash's own jobs (process groups) | **Complete** | Section 11 below; spec D13, D21 |
| 12 | A path a script can exec for a bundled tool | **Complete** | Section 12 below; [open issue 4](open-issues.md) |
| 13 | `bc` against GNU bc | **Complete** | Section 13 below; spec D56 |
| 14 | Bash 5.3 remainder without a terminal: `wait -n` in POSIX mode, `array_expand_once`, the `RETURN` trap's status | **Complete** | Section 14 below; [Bash 5.3 audit](research/bash-reference/bash-5.3-audit.md) |
| 15 | ConPTY probe harness, then the interactive and Readline 5.3 items | **Complete** (cash reports `BASH_VERSION=5.3.15`) | Section 15 below |
| 16 | `cash --link-tools`: hard links so programs outside cash can run its tools | **Complete** | Section 16 below; spec D65 |
| 17 | What fish has at the prompt: highlighting, bash's Alt-. and Ctrl-X Ctrl-E, `abbr`, a collapsing prompt, folder history, carapace completions | **Complete** | Section 17 below; spec D59–D63 |
| 18 | Installing cash: a Scoop bucket now, winget with an Inno Setup installer later; the tool links on the user PATH on request | **Active** (Scoop since 1.1.0; the per-user installer, the portable zip's first-prompt offer and `cash --update` since 1.6.0; winget manifests validated, submission to come) | [Packaging evaluation and decisions](research/packaging-evaluation.md); section 18 below; spec D38, D65 |
| 19 | Console, clipboard and the remaining small tools: `tput`, `stty`, `iconv`, `column`, `xxd`, `hexdump`, `uuidgen`, `xdg-open`, `pbcopy`/`pbpaste`, `watch`, `free`, `nice`/`renice`, `flock`, `nc` | **Complete** (1.5.0, 2026-10-06) | Section 19 below; spec D74; DONE.md phase 21 |
| 20 | `grep`, `diff` and `cmp`: the standing rule reversed under "install cash, have everything" | **Complete** (1.7.0, 2026-10-06) | DONE.md phase 23; spec D76 |
| 21 | `gzip`, `gunzip` and `zcat` in pure Rust | **Complete** (1.8.0, 2026-10-06) | DONE.md phase 24; spec D77 |
| 22 | Compatibility corners: `/dev/tcp`, `command_not_found_handle`, kept `abbr`, kinder refusals, `getconf`, `locale` | **Complete** (1.8.0, 2026-10-06) | DONE.md phase 25; spec D77 |
| 23 | Archive and compression tools on shared parts: `bzip2`, `xz`, `zstd` and their names, `tar`, `zip`/`unzip`/`zipinfo` | **Complete** (1.9.0, 2026-10-07: the compressors, `tar`, and `zip`, `unzip`, `zipinfo`) | [Design](research/archive-tools-design.md); DONE.md phases 29–30; spec D78 |
| 24 | The prompt and paths: a Ctrl-R history picker, the `z` folder jump, every path spelling everywhere, and a guard on size and speed | **Complete** (1.12.0, 2026-10-10) | [Design](research/prompt-history-and-jump-design.md); DONE.md phase 34; spec D79, D80 |
| — | Found on the way (not planned items) | **Complete** | MSYS2 argument encoding (spec D52); `shopt winpaths` and bash-worded `cd` errors (D53); a `TERM` that no longer reaches the whole console (D21) |

The authoritative feature order is therefore:

```text
Bash 5.2 completion (done)  ->  native awk (done)  ->  native sed (done)  ->  bundled userland (done)
  ->  Bash 5.3 (done)  +  corpus fixes (active)  ->  line-ending controls (done)
  ->  fuser / lsof subset (done)  ->  ss subset (done)  ->  BusyBox-gap tools (done)
  ->  graceful signals for own jobs (done)  ->  exec path for bundled tools (done)
  ->  bc against GNU bc (done)  ->  Bash 5.3 remainder (no terminal) (done)  ->  ConPTY harness + 5.3 interactive (done)  ->  tool links
  +  fish's prompt conveniences (done)  ->  installing cash: Scoop, then winget (active)
  ->  console, clipboard and the remaining small tools (active)
```

## 1. Finish Bash 5.2

Cash reports `BASH_VERSION=5.2.37`, so closing or explicitly classifying its 5.2 gaps is
the first feature priority. Work through the existing plan in its own phase order:

1. small portable semantics: here-documents, `ulimit` parsing, `command -p`, startup
   `$0`, empty-word descriptor duplication, and fatal parameter transformations;
2. array, subscript, `unset`, and nameref evaluation edge cases;
3. completion and interactive editing, including globstar completion, `read -E`, history
   navigation, and the separately scoped custom-keymap work;
4. variable file-descriptor redirection and `varredir_close`;
5. the Bash 5.2 POSIX-mode pass;
6. native Windows coverage for CRLF, ConPTY, paths, and handles. A Linux GitHub Actions
   job diffed focused probes against a real Bash 5.2 until 2026-09-28, when cash became
   Windows-only (spec D43).

The [remaining plan](research/bash-reference/bash-5.2-remaining-plan.md) owns the probe
matrix and implementation sequence. The [gap audit](research/bash-reference/bash-5.2-gaps.md)
owns observed results and status.

This workstream is complete when every listed 5.2 NEWS item and older advertised-interface
gap is recorded as one of:

- verified compatible with a focused regression;
- fixed with a focused regression;
- a documented, deliberate Windows divergence;
- a non-applicable Bash build or library detail.

“Needs investigation” does not count as complete. A deliberate platform divergence may
close an item; exact Unix behavior is not required where Win32 lacks the underlying
concept.

## 2. Add native `awk`

Start this only after the Bash 5.2 completion gate above. Use the reviewed
[`posixutils-rs`](https://github.com/rustcoreutils/posixutils-rs) AWK source as a one-time
import and maintain the resulting code inside Cash. Do not import the full utility suite,
create an upstream-tracking fork, or add a workspace Git dependency.

The implementation sequence is:

1. create a private `crates/cash-awk` crate, preserve the MIT notice and exact imported
   revision, and retain the 403 upstream AWK tests;
2. split its binary setup from the parser/compiler/interpreter and expose a non-exiting
   entry for Cash's process-backed bundled-command shim;
3. replace the libc POSIX-regex wrapper with a Windows-capable adapter that preserves
   earliest/longest ERE matching;
4. replace libc `system`/`popen`/`pclose` with a Cash command host for `system()`, command
   `getline`, and output pipes;
5. replace the narrow locale dependencies and define CRLF record behavior;
6. fix the verified array/`ARGV` and function-argument panics, then audit reachable unsafe
   stack and array paths so script errors cannot unwind;
7. add Windows and Linux differential tests before registering the command as `awk` and
   updating `cash doctor`, README, and the specification.

The detailed evidence and gate are in
[research/posixutils-rs-evaluation.md](research/posixutils-rs-evaluation.md). Complete gawk
extensions remain outside the initial gate.

## 3. Add native `sed` (Complete & Verified)

`crates/cash-sed` is integrated from the reviewed `uutils/sed` source, hardened, and verified.
POSIX earliest/longest regex matching, BRE backreferences in default Windows mode, `n` cycle behavior,
ordinary GNU `-i`, trailing empty file `$`, combined numeric/`g` flags, CRLF preservation, and `cash -c`
execution are verified with 360 unit tests, 271 integration tests, and the sed cases of the
differential corpus, frozen from Git Bash's GNU sed and checked in `it` (`tests/corpus/sed`,
`it/corpus_goldens.rs`).

## 4. Expand bundled userland (Complete & Verified)

All planned high-impact core utilities are implemented, integrated, and verified:

1. **`stat`**: Native Windows `stat` implemented and verified (`stat -c %s`, `%F`, `%a`, `%Y`, etc.), with full human-readable and custom specifier support. Removes the `stat` warning in `cash doctor`.
2. **`tty`**: Native Windows `tty` implemented using `context.try_fd(0).is_terminal()`. Outputs `/dev/tty` (code 0) or `not a tty` (code 1), supporting `-s` / `--silent`.
3. **`install`**: Native Windows `install` implemented supporting `-d` (recursive directory creation), `-D`, `-m` (modes), `-t` (target directory), `-C` (content compare), and `-v` for Makefiles and build scripts.
4. **`pathchk`**: Integrated `uu_pathchk` via `cash-coreutils-builtins` for POSIX path portability checks.
5. **`nohup`**: Native Windows `nohup` implemented with detachment/process group creation, terminal stdin redirection from null, and stdout/stderr appending to `nohup.out`.
6. **`who` & `users`**: Implemented session query via Windows `quser` and session environment queries, reporting active sessions and user lists.
7. **`pinky`**: Implemented user query tool reporting user directory and session state.
8. **`logname` & `hostid`**: Implemented `logname` (login user via `%USERNAME%`) and `hostid` (32-bit hex host identifier).

All verified with focused integration tests in `crates/cash/tests/identity_and_jobs.rs` and `resolution_honesty.rs`.

## 5. Bash 5.3 compatibility work (Active)

Cash already contains a few selected Bash 5.3 behaviors, including current-shell command
substitution, `compgen -V`, and `source -p`. They remain supported, but they do not change
the ordering or constitute a Bash 5.3 version claim.

After Bash 5.2, `awk`, `sed`, and the bundled userland expansion are complete, create a
dedicated Bash 5.3 NEWS/source audit before adding more features. Classify the results into:

1. portable language and builtin behavior;
2. interactive completion, Readline, and job-control behavior;
3. POSIX-mode behavior changes;
4. Bash build internals that do not create a Cash-visible contract;
5. deliberate Win32 divergences.

Prioritize small, useful script-visible features that fit Cash's architecture. Larger
POSIX-mode or editor changes require focused designs and differential tests. Do not report
`BASH_VERSION=5.3` until the audit is complete and every applicable item has a final
classification.

**Status: complete.** Every item in the audit is implemented, verified or documented:
the 39 script probes (`research/bash-reference/probe.ps1 -Suite 53`) and the ConPTY
harness's interactive cases (item 15) match the Bash 5.3.15 oracle, and cash reports
`BASH_VERSION=5.3.15`.

## 6. Differential corpus

`tests/corpus` holds sourced, copied scripts (Pement's sed and awk one-liners, the GNU sed
manual's sample scripts, Wooledge BashPitfalls and BashFAQ, pure-bash-bible), run by
`tests/corpus/run.ps1` under Cash and under Git Bash 5.3 with GNU sed and gawk. Cash's sed and
awk target POSIX, so each mismatch is classified as a Cash bug, a GNU/gawk-only extension, or
a Windows divergence. The first run (209 cases) found 19 mismatches, among them an awk
compiler panic on `for` loops with an empty clause.

**Status:** 205 of 209 cases match. Each fix has a focused regression; besides those named in
the commit history, the corpus exposed `(( count[$word]++ ))` storing every word under key
`0` (associative subscripts were evaluated as arithmetic), `for x in; do` iterating over
`$@`, and `sed`'s `l` ignoring `-l`. The four remaining cases are classified in their files'
`# tags:` lines:

- deliberate divergences: arithmetic never runs `$(...)` from a variable's value (spec §4,
  row 27), and `s/.$//` does not see a hidden CR (D49);
- GNU-only sed extensions: `\b` word boundaries, a label ended by a space.

The last gap closed was pure-bash-bible's `reverse_array`, which reads `BASH_ARGV` under
`extdebug`. cash built `BASH_ARGV` from its call stack; Bash keeps a stack of its own. A
function pushes its arguments there only when `extdebug` is on at the call, `source` pushes
its file's name, and `$@` goes on once, when `extdebug` is first turned on or a command
outside any function first reads the variable. cash now keeps that stack, and matches Git
Bash 5.3.15 in scripts, `-c` and standard input, where it had shown nothing.

Keep the corpus growing and fix new mismatches with focused regressions.

## 7. Line-ending controls

The corpus showed that the classic dos2unix one-liners (`sed 's/\r$//'`, `sed 's/.$//'`)
cannot work with Cash's sed, which strips the CR before matching and restores it on output.
Agreed design:

1. **Default (unchanged in spirit):** CRLF input stays CRLF on output and `$` matches before
   the CR, so ordinary edits of Windows files work. When a command's pattern explicitly names
   a carriage return (`\r`, `\x0D`, a literal CR), it is matched against the real line
   including the CR, so the dos2unix one-liners do what they say.
2. **Linux mode:** CR is ordinary data and only LF ends a line, exactly as GNU sed on Linux.
   Selected per run with `-b`/`--binary` (the option GNU sed's Windows builds use for this),
   or for a whole session or script through an environment variable.
3. **`dos2unix` and `unix2dos` builtins,** following the real tools' manual: in-place
   conversion by default, `-n INFILE OUTFILE` pairs, stdin-to-stdout filtering, `-k` (keep the
   date), `-q`, `-f`, skipping binary files unless forced, and their BOM defaults. A bundled
   builtin shadows Git for Windows' `usr/bin/dos2unix.exe` when that is on `PATH`, so
   `type`, `cash doctor` and `enable -n` must report and allow the fallback honestly.
   `mac2unix`/`unix2mac` (CR-only files) are not planned.

**Status: complete.** sed and awk implement the policy with the shared `CASH_EOL=lf`
switch (spec D49); an explicit CR in the program is detected per run rather than per
command. awk also now splits default fields on space, tab and newline only, as POSIX
requires. `dos2unix`/`unix2dos` were checked against dos2unix 7.5.6 from Git for Windows;
code-page and UTF-16 conversion are refused with a pointer to `enable -n`. Tests:
`cash-sed` `test_crlf_*`, `cash-awk` `test_eol_*`, `crates/cash/tests/line_ending_tools.rs`.

## 8. `fuser` and an `lsof` subset

Agreed scope: `fuser` in full, `lsof` as a documented subset.

1. `fuser FILE` and `lsof FILE` through the Restart Manager (`RmStartSession`,
   `RmRegisterResources`, `RmGetList`), the documented API behind Explorer's
   "file in use" dialog;
2. `fuser PORT/tcp|udp` and `lsof -i[:PORT]` through `GetExtendedTcpTable` and
   `GetExtendedUdpTable` (IPv4 and IPv6, owning PID);
3. `lsof -p PID` limited to what those sources answer, `fuser -k` on the existing `kill`
   support, and `-v` listings;
4. anything needing a system-wide handle walk (undocumented `NtQuerySystemInformation`
   handle enumeration) is refused with a clear message rather than approximated.
   Superseded on 2026-10-04 (TODO phase 17): `lsof` walks the handles as handle.exe does,
   for bare `lsof` and the open files of `-p`, `-c` and `-u` (spec D50).

**Status: complete.** Decisions taken while building it: a directory argument means the
files below it (the Restart Manager cannot see working directories); lsof keeps all nine
columns with `-` where Windows has no value; `lsof -p` shows the executable, modules and
sockets with a note on standard error. The socket layer is `cash_win32::net`, ready for
item 9. Tests: `crates/cash/tests/fuser_lsof.rs`, plus unit tests in `cash-win32` (`net`,
`restart`, `process`) and `cash-builtins` (`fileuse`). Found on the way and filed
separately: after `exec 3>file`, external commands fail (D26 over-applies).

## 9. `ss` subset

A Linux-style `ss` on the socket layer built for item 8, written directly on `windows-sys`
in `cash-win32`. Supported: `-t -u -l -a -n -p -4 -6 -f -H -O -s -F`, state filters and a
subset of the address/port expressions. `Recv-Q`/`Send-Q` print `0`; `-p` adds
`service=NAME` for svchost PIDs; `-i` and the options with no Windows backing are refused,
and netstat-style flags (`-ano`) get a hint with the `ss` spelling. Windows' own
`netstat.exe` is not shadowed. Decisions and the full option matrix:
[research/ss-evaluation.md](research/ss-evaluation.md).

**Status: complete.** Checked against iproute2 7.2 in WSL for `-tulpn`, `-tlnp`, `-tan`,
`-uan`, `-tunap`, `-H`, `-Q`, `-4`/`-6`, state filters and `-s`. `service=` comes from
the socket's owning module (`GetOwnerModuleFrom*Entry`), which names the exact service
even when one `svchost.exe` hosts several. Tests: `crates/cash/tests/ss.rs` and the filter
unit tests in `cash-builtins`.

## 10. BusyBox-gap tools

From the [BusyBox gap analysis](research/busybox-gap-analysis.md), the agreed set:

1. `pkill`, `pidof`, `killall` on the existing `pgrep` matcher and `kill` signal path:
   case-insensitive names, `.exe` optional, access-denied and system processes skipped;
2. `getopt` (util-linux interface; its output quoted for Cash's parser), `rev` (keeps a
   trailing CR), `clear` and `reset`;
3. POSIX `bc`, imported from posixutils-rs and hardened like `awk`;
4. a Linux-flag `ping` over the ICMP helper APIs, with pure iputils flags; it shadows
   `ping.exe` deliberately, so `type`, `cash doctor` and `enable -n` must report it;
5. `cash doctor` flags BusyBox shims on `PATH` for the tools whose BusyBox versions are
   known to break scripts.

Not adopted: `grep`, `cmp`/`diff` and the gzip family; Cash still does not carry grep or diff.

**Status: 1 done** (spec D54), checked against procps-ng 4.0.7 and psmisc 23.7 in WSL;
`pgrep` now shares the family's name rules. Building it exposed that `kill -TERM` (and
`HUP`, `QUIT`, `INT`) sent a console control event that reached every process on the
console, the shell included, since cash starts no child as a process-group leader. `TERM`
now asks through the target's windows and escalates (D21). Starting cash's own children
in groups of their own, for a graceful console event, is left for a design of its own
because it changes Ctrl-C for every foreground program (D13).

**2 done** (spec D55): `getopt` and `rev` match util-linux 2.42.3 case by case
(`crates/cash/tests/oracle`), apart from `-s csh`/`tcsh` refused and a CRLF line's `\r`
kept at the end; `clear` and `reset` write ncurses 6.6's bytes, and `reset` also restores
the console modes.

**3 done** (spec D56): POSIX `bc` from posixutils-rs as `crates/cash-bc`, bundled like
`awk`. Upstream's 72 tests run through cash; cash adds CRLF input, no leading zero
(`.33`, as GNU/BSD/BusyBox print), GNU's harmless options, and a note naming any GNU
extension a failed program used.

**4 done** (spec D57): `ping` with iputils' flags and output over `IcmpSendEcho2`, bundled
so Ctrl-C prints its statistics; `ping.exe` by path or `enable -n ping`; doctor reports
the shadow, and `ping -n 3 host` gets a hint instead of an endless ping.

**5 done** (spec D35): doctor flags the BusyBox applets that break scripts — `grep`,
`diff`, `make`, `tar`, `xz`, `nc`, `wget`, each reason checked against BusyBox 1.38 — and
points at a full copy later on `PATH` when there is one. Item 10 is complete.

## 11. Graceful Ctrl-C and `TERM` for cash's own jobs

Cash starts no child as a process-group leader, so a console control event cannot be
aimed at one of its jobs: aimed at a non-leader it reaches the whole console. `TERM`
therefore asks through a program's windows and terminates a console program at once
(D21). Giving cash's jobs process groups of their own would let `kill -TERM %1` and
`kill -INT $!` deliver a real CTRL_BREAK a console program can handle — but a new group
also stops the keyboard's Ctrl-C from reaching it, so the change touches how every
foreground program is interrupted (D13). Design first: which jobs get their own group
(background only, or all), how Ctrl-C reaches a job in its own group, what `fg`/`bg`
do, and what programs that treat Ctrl-C and Ctrl-Break differently (Python) see.

**Done** (spec D13 "As built", D21): background jobs at the prompt lead groups of their
own, so Ctrl-C at the prompt no longer kills them; `kill -TERM` sends them Ctrl-Break
then terminates after 5 s; after `fg`, Ctrl-C is relayed as Ctrl-Break and a second one
ends the job. Foreground commands and scripts are unchanged, and an interactive cash
takes back Ctrl-C a parent ignored. Tests: `crates/cash/tests/job_groups.rs`.

## 12. A path a script can exec for a bundled tool

`LS=$(which ls); "$LS" -la` gets an empty command, because `ls` is a builtin with no file
behind it ([open issue 4](open-issues.md)). `cash --invoke-bundled ls` is the real
executable, and nothing says so. Decide between reporting it in `which`/`type`, shipping
shim executables, or another answer.

**Done** (spec D58): `which ls` prints `C:/…/cash.exe/ls`, a virtual path under cash's own
executable. Cash runs it as `ls` — typed, via `exec`, `xargs` or `find -exec` — and
`[ -x "$LS" ]` is true; no launcher files. Bash's own builtins still say `shell builtin`.
Programs outside cash cannot run the path.

## 13. `bc` against GNU bc

The imported suite was checked against GNU bc upstream, but not here: the WSL distros have
no GNU bc, and installing one needs the user's password (`wsl -d kali-linux sudo apt
install -y bc`). Once it is there, run a differential corpus as for `awk` and `sed`,
starting with what could not be checked: the line-wrap column of long numbers, `obase`
above 16, and the math library at high scale.

**Done** (spec D56): GNU bc 1.07.1, unpacked without a password (`apt-get download bc`,
`dpkg -x`), ran the 52 cases in `crates/cash/tests/oracle/bc_cases.sh`. Every number
matched: the 70-column wrap, `obase` 17 to 1000, and `e`, `a`, `l`, `s`, `c`, `j` at
scale 25 to 50. Two differences came up: a one-line `define f(x) { ... }`, now accepted
as GNU, BSD and BusyBox accept it; and the exit status after an error, 1 in cash and 0
in GNU, kept on purpose. Tests: `bc_matches_gnu_bc` in `crates/cash/tests/bc_cash.rs`.

## 14. Bash 5.3 remainder without a terminal

The audit items a `-c` script can observe that are not yet done: `wait -n` in POSIX mode,
indexed `array_expand_once`, and the status the `RETURN` trap sees before `return`. Each
gets a probe against Git Bash 5.3.15, as the other 35 have.

**Done:** 39 of 39 probes match. The `RETURN` trap sees `$?` from before `return`;
builtins expand a quoted subscript again (`unset 'a[$i]'`) unless `array_expand_once` is
set, but never run a command substitution there (spec divergence 37, decided); `wait`
keeps finished jobs' statuses, and `wait -n` forgets them only in POSIX mode. On the
way: arithmetic errors in builtins abandon the line as in Bash, `(( 1+ ))` no longer
does, and `{ exit 3; } &` no longer exits the shell that waits for it. Tests:
`crates/cash/tests/bash_gaps.rs`.

## 15. ConPTY probe harness, then the interactive 5.3 items

The interactive and Readline items of the 5.3 audit need a real terminal: a harness that
drives cash through ConPTY, sends keys and reads the screen, alongside the same session in
Bash 5.3. Then the items themselves, and with them the `BASH_VERSION=5.3` claim.

**Done:** cash reports `BASH_VERSION=5.3.15(1)-release`. The harness
(`crates/cash/tests/pty_oracle.rs`) replays each case's keys in a ConPTY and compares the
screen cash leaves with Git Bash 5.3.15's; 36 cases, none left unfixed, the deliberate
differences pinned to cash's own screen. It found far more than the 5.3 items: completion
quoted every name with a space twice, `bind -x` output was painted over (a patch to the
carried Reedline), `read -t` never timed out at the console, job notices were laid out and
timed differently and sometimes lost, `COLUMNS` and `LINES` were never set, and `kill $$`
ended cash, trap or no trap. Decided on the way: completion quotes in the style the word
was started in (D40), and `CHLD` is emulated (D64). Last, a stale test note led to a 5.3
change the audit had missed: `BASH_SOURCE` names `$0` for what `-c` and standard input
define.

## 16. `cash --link-tools`: tools that programs outside cash can run

D58's `C:/…/cash.exe/ls` runs only inside cash; Python's `subprocess` or a `.bat` file
can start only a real file. `cash --link-tools [DIR]` makes hard links to `cash.exe`,
one per tool (`ls.exe`, `grep.exe`, …), and cash started under a tool's name runs as
that tool, as BusyBox does. A hard link is the same file, so it costs no space and needs
no admin on a folder the user can write. Under AppLocker or App Control, only an
installer running as admin can put the links where they may run, next to `cash.exe` in
`Program Files`. The links have the same hash and signature as `cash.exe`, so rules
allowing cash by hash or signer allow its links.

Decided:

- **Folder:** DIR, or `bin` next to `cash.exe`, created when missing; an existing `bin`
  is used as it is (busybox-w32 `--install` likewise defaults to its own folder).
- **A name already taken:** a link cash made earlier is refreshed to this `cash.exe`,
  so re-running after an upgrade updates them; any other file is left alone and listed
  as skipped. Cash knows its own links by a list it keeps in the folder.
- **Windows' names:** all 126 tools are linked (125 when this was decided; `where`
  came later, D66). The nine that share a System32 name (`expand find hostname ping
  reset sort timeout where whoami`) get a note: put the folder
  after System32 on PATH, or `.bat` files calling `find` or `sort` get cash's.
- **PATH:** never changed; the command prints the folder and how to add it to the
  user PATH.

Also: a DIR on another drive cannot hold hard links to `cash.exe`, so that is an error
naming the reason; `which` prints a tool's link, when one is on PATH, in place of the
virtual path; and `cash doctor` reports links left pointing at an older `cash.exe`.

Done as decided (spec D65). Two things surfaced on the way. Windows looks for a program
started by a bare name in the starting exe's own folder first, so cash, coming up as any
tool in a links folder, started the `whoami.exe` beside it for the user's SID, which was
cash again: every start started another. cash now starts the Windows programs it uses by
their System32 path. And a linked tool that re-enters its own exe (a bundled tool, `sh`)
would take the child for the tool again; the child knows it by an environment variable
naming the link. Testing `xargs.exe -n1` also found `xargs` reading `-n1`, with its count
attached, as the command to run.

## 17. What fish has at the prompt

A comparison with fish (Garuda's default interactive shell) listed what it offers at the
prompt that cash did not. Decided, in this order:

1. **Syntax highlighting on by default** (D59). It existed, off; turning it on needed a
   background `PATH` listing, since probing `PATH` for a missing name took 127 ms a
   keystroke.
2. **Bash's own Alt-. and Ctrl-X Ctrl-E**: `yank-last-arg` was defined but bound to
   nothing, and `edit-and-execute-command` had no editor behind it. Bash parity, not fish.
3. **`abbr`, fish-style**: `abbr -a gco git checkout`; the command word expands on Space
   or Enter, so history keeps the full command. A cash-only builtin (D45, D60).
4. **A collapsing prompt**: off unless `CASH_TRANSIENT_PS1` is set; it takes PS1's
   escapes. For Starship, `$(starship module character --status="$STARSHIP_CMD_STATUS")`,
   about 30 ms a prompt (D61).
5. **Folder history**: Alt-← and Alt-→ on an empty line step through visited folders, as
   in fish, with `prevd`, `nextd` and `cdh`. Windows Terminal takes Alt-arrows while
   panes are split (D62).
6. **Completions from carapace, when it is installed**: for a command with no completion
   of its own, with carapace's descriptions in the menu. Not bundled: carapace is Go, 90 MB
   unpacked. Measured here, a Tab costs 85–100 ms for carapace's static completers and
   220–450 ms where it runs the tool (git, docker), no slower than the tools' own bash
   scripts; nothing runs per keystroke (D63).

**Status: complete.** Found on the way, each fixed with a regression test:

- cash's awk rejected `break` inside `for (k in a)` and `do … while`, which broke
  `git checkout <Tab>` in Git's own completion script;
- the highlighter treated only a line's first word as a command, so `ls | nosuch` never
  marked `nosuch`;
- Reedline expanded an abbreviation only when a read began with the space (patch 4), and
  dropped keys typed straight after a `bind -x` key (patch 5), which hit atuin and fzf
  users too.

Closed by item 18: the Scoop manifest offers carapace at install time through `suggest`;
`cash doctor` still names it when it is missing. The winget manifest is still to come.

## 18. Installing cash: Scoop now, winget later

The question began with `cash --link-tools`: is it tested, and should cash put its folder
on PATH? It widened to installing cash as a normal user, without admin. The research,
with sources, is in [the packaging evaluation](research/packaging-evaluation.md).
Decided with the user, 2026-09-28:

- **Scoop only, for now**, from `tomcoolpxl/scoop-bucket`, a bucket of the author's own
  that its scheduled action keeps current. winget follows, with an **Inno Setup**
  per-user installer: a portable winget package cannot upgrade while `cash.exe` runs,
  and the installer type cannot change once published. Chocolatey is not planned; its
  normal setup needs admin.
- **The tool links on PATH are opt-in**, and go at the **front of the user PATH**, where
  they win over Scoop's BusyBox shims while System32's `find`, `sort` and `timeout` still
  win for `.bat` files. **cash writes PATH itself**, keeping `REG_EXPAND_SZ` and `%VAR%`
  entries, in place of the PowerShell line it printed, which flattened them.
- **A Windows Terminal profile** is written on every install and removed on uninstall.
- **`tomcoolpxl/cash` goes public**, so its release zips can be downloaded.
- **Unsigned for now**; the SignPath Foundation once cash has visible users.

Work, in order:

1. **Done.** `cash.exe` links the C runtime statically (it needed `VCRUNTIME140.dll`), and
   the release checks it; the zip carries `NOTICE`, `licenses/` and
   `THIRD-PARTY-LICENSES.html` from `cargo about` (466 crates).
2. **Done.** An icon and a version resource in `cash.exe`.
3. **Done.** `--link-tools --add-to-path` and `--unlink-tools` (D65). A link Windows
   will not delete is renamed aside instead of stopping the refresh halfway.
4. **Done.** `--terminal-profile` and `--remove-terminal-profile` (D38).
5. **Done.** [`packaging/scoop/cash.json`](packaging/scoop/cash.json), the README's
   install section, and RELEASING's Scoop step.
6. **Done, 2026-09-28.** `tomcoolpxl/cash` is public; 1.1.0 is released with these
   commands; [`tomcoolpxl/scoop-bucket`](https://github.com/tomcoolpxl/scoop-bucket),
   made from Scoop's bucket template, carries `bucket/cash.json` at 1.1.0, and its
   Excavator workflow updates it from new releases. `scoop bucket add tomcoolpxl
   https://github.com/tomcoolpxl/scoop-bucket` then `scoop install cash` installs it.
7. **Decided 2026-10-06, next** (DONE.md phase 22, spec D75): the Inno Setup per-user
   installer on the releases page, with Scoop's versioned layout and `current` junction,
   the tool links on PATH by default, `cash --update` for upgrades on request, and the
   winget submission from that installer. Unsigned; signing is postponed (no paid
   certificate, SignPath not wanted).

Found at the release: GitHub's Windows runners run elevated, so `$EUID` is 0 there and
new files belong to Administrators; two tests that assumed a normal user failed there
only, and now expect either.

Found on the way: Windows deletes a name of a running program while the file has others,
and refuses only the last; the tests had been leaving 126 links per folder behind, until
`cash.exe` reached NTFS's 1023 names; and Scoop's `suggest` now offers carapace at install
time, the gap left open in item 17.

## 19. Console, clipboard and the remaining small tools

A second look (2026-10-06) at what a clean Windows machine with cash still lacks, after
the [BusyBox gap analysis](research/busybox-gap-analysis.md) adopted its tier 1 (item
10). Every name below resolves to nothing on a bare Windows install, none belongs to a
program in System32, and each either has to agree with something cash owns (the console
modes, encoding, the clipboard, the descriptor table, process ids, command resolution)
or is cheap and collides with nothing. Chosen by the user on 2026-10-06; the decisions
are spec D74 and the plan DONE.md phase 21.

1. `tput` and `stty`: the console, with ncurses' sequences and GNU's words;
2. `iconv`: every Windows code page, glibc's options and messages;
3. `column`, `xxd`, `hexdump`: text and bytes, oracle-tested against util-linux and vim;
4. `uuidgen`, `xdg-open`, `pbcopy`/`pbpaste`: small, and the clipboard as UTF-8;
5. `watch`, `free`, `nice`/`renice`, `flock`, `nc`: processes, memory, locks and ports.

Not adopted, again: `grep`, `diff`, `cmp`, the compressors, `patch`, `strings`.

## Keeping the roadmap current

When work starts or finishes:

1. update the status table in this file;
2. update the linked detailed plan or audit with probes, implementation status, and tests;
3. update README/spec only when user-visible behavior actually changes;
4. move newly discovered work into the applicable detailed plan instead of leaving it in
   chat history or an isolated note.

This file owns **what comes next**. The linked documents own **how each workstream is
implemented and verified**.
