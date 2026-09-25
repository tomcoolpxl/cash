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
| 5 | Bash 5.3 compatibility work | **Active** | [Bash 5.3 audit](research/bash-reference/bash-5.3-audit.md): portable and POSIX-mode items probed against Bash 5.3.15 |
| 6 | Differential corpus fixes | **Active** | [`tests/corpus`](tests/corpus): sourced sed/awk/Bash one-liners run against Git Bash 5.3 with GNU sed and gawk |
| 7 | Line-ending controls: sed `-b` and `dos2unix`/`unix2dos` | **Next** | Section 7 below |
| 8 | `fuser` and an `lsof` subset | Planned | Section 8 below |
| 9 | `ss` subset | Planned | [`ss` evaluation and decisions](research/ss-evaluation.md) |

The authoritative feature order is therefore:

```text
Bash 5.2 completion (done)  ->  native awk (done)  ->  native sed (done)  ->  bundled userland (done)
  ->  Bash 5.3 (active)  +  corpus fixes (active)  ->  line-ending controls
  ->  fuser / lsof subset  ->  ss subset
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
6. a Linux GitHub Actions differential job using an actual Bash 5.2 executable, while
   retaining native Windows coverage for CRLF, ConPTY, paths, and handles.

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
execution are verified with 360 unit tests, 271 integration tests, and 30 differential tests against
GNU sed in WSL (`tests/sed-differential.sh`).

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

**Status.** The audit exists, and every portable and POSIX-mode item that a `-c` script can
observe has a probe (`research/bash-reference/probe.ps1 -Suite 53`); all 35 match the Bash
5.3.15 oracle. The audit lists what remains before a version claim: the interactive and
Readline items (they need ConPTY probes), real-signal traps, `wait -n` in POSIX mode,
indexed `array_expand_once`, and the `RETURN` trap's pre-`return` status.

## 6. Differential corpus

`tests/corpus` holds sourced, copied scripts (Pement's sed and awk one-liners, the GNU sed
manual's sample scripts, Wooledge BashPitfalls and BashFAQ, pure-bash-bible), run by
`tests/corpus/run.ps1` under Cash and under Git Bash 5.3 with GNU sed and gawk. Cash's sed and
awk target POSIX, so each mismatch is classified as a Cash bug, a GNU/gawk-only extension, or
a Windows divergence. The first run (209 cases) found 19 mismatches, among them an awk
compiler panic on `for` loops with an empty clause; the "Complete" status of the awk and sed
workstreams above is subject to these fixes. Fix bugs with focused regressions and keep the
corpus growing.

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

## 9. `ss` subset

A Linux-style `ss` on the socket layer built for item 8, written directly on `windows-sys`
in `cash-win32`. Supported: `-t -u -l -a -n -p -4 -6 -f -H -O -s -F`, state filters and a
subset of the address/port expressions. `Recv-Q`/`Send-Q` print `0`; `-p` adds
`service=NAME` for svchost PIDs; `-i` and the options with no Windows backing are refused,
and netstat-style flags (`-ano`) get a hint with the `ss` spelling. Windows' own
`netstat.exe` is not shadowed. Decisions and the full option matrix:
[research/ss-evaluation.md](research/ss-evaluation.md).

## Keeping the roadmap current

When work starts or finishes:

1. update the status table in this file;
2. update the linked detailed plan or audit with probes, implementation status, and tests;
3. update README/spec only when user-visible behavior actually changes;
4. move newly discovered work into the applicable detailed plan instead of leaving it in
   chat history or an isolated note.

This file owns **what comes next**. The linked documents own **how each workstream is
implemented and verified**.
