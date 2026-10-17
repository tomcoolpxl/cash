# cash — Cool Again Shell

**Status:** early design, internally consistent as of the 2026-09-21 audit.
Decisions are settled unless marked OPEN. Background reasoning lives in
[musings.md](musings.md); this file is the spec.

---

## 1. Premise

cash is a **bash-language shell whose execution model is Win32**, hosted in Windows
Terminal via ConPTY. It is not a POSIX emulation layer and it is not a terminal
emulator.

The target user is someone running ordinary-complexity bash scripts on Windows —
Terraform wrappers, CI glue, pipelines with `xargs`, `sed`, `grep`, `jq` — who does
not want WSL, a VM, or an `msys-2.0.dll` underneath.

**Non-goal:** running arbitrary Linux bash scripts with perfect fidelity. That road
ends at reinventing Cygwin.

### Daily-driver requirements

cash has to be pleasant enough to replace the user's current shell, not merely correct.
Stated requirements, as opposed to nice-to-haves:

- **Starship must work.** The user runs it on every machine and every shell. A hard
  requirement, not a compatibility bonus — see D37.

---

## 2. Landscape (as of 2026-09)

Two things shipped that change the design:

| Piece | State | Consequence for cash |
|---|---|---|
| [brush](https://github.com/reubeno/brush) | MIT, Rust, bash/POSIX-compatible, ~2,500 differential tests vs bash, modular crates, Windows in preview | The shell *language* is largely solved. cash should not rewrite it. |
| [Coreutils for Windows](https://github.com/microsoft/coreutils) | Microsoft-maintained, native Win32, Rust uutils + findutils (`find`, `xargs`) + GNU-compatible `grep`, on winget | Most of the *userland* is solved — but not all of it. See D35. |

Note the second row carefully: it is **not** a complete userland. MS Coreutils ships no
`sed` and no `awk`, because those are separate GNU projects. D35 covers the consequences.

**And note what cash bundles, which is a narrower thing.** That row describes a package a
user may install, not something cash carries. D48 bundled **uutils coreutils only** at
first — there is no `uu_find`, `uu_xargs` or `uu_grep` on crates.io, so `find`, `xargs`,
`grep`, `sed`, `awk`, `diff` and `stat` were *not* in the binary. Measured then with
`PATH` reduced to `C:\WINDOWS\system32;C:\WINDOWS`: 65 of 103 expected commands resolved,
34 were absent, and 4 were shadowed by unrelated Windows tools of the same name. cash's
own M2 corpus script failed on such a machine — loudly, with exit 2, because
`set -euo pipefail` caught it, but only after DOS `find` had produced
`File not found - *.rs`. cash has since come to carry `find`, `xargs`, `sed`, `awk`, `bc`
and `stat` of its own; `grep` and `diff` are still not in the binary (2026-10-04).

So **Git for Windows, or Microsoft's Coreutils, is a prerequisite for the §1 workload**,
and `cash doctor` says so. The three utilities cash does own that would otherwise come
from there — `ps`, `less`/`more`, `chmod` — are carried because their stand-ins were
worse than absent: two reported numbers and permissions that nothing else on the system
agreed with, and the third corrupted a pipe.

Two premises from `musings.md` are revised:

- **`fork()` is not the central problem.** A Rust shell cannot safely `fork()` on any
  platform (threads + async make post-fork state undefined), so any Rust shell already
  implements subshells, `$(...)` and pipeline stages as in-process cloned shell state.
  Choosing Rust deletes that entire chapter.
- **Process-tree kill is a Windows *strength*, not a gap.** On Linux, killing bash does
  not reliably kill its descendants — they reparent to init and survive; cleanup relies
  on cooperative SIGHUP-to-process-group, defeated by `nohup` / `setsid`. Windows
  [job objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
  are kernel-enforced: a process cannot leave its job, children join automatically, and
  `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` reaps the whole tree. Nesting has worked since
  Windows 8, so cash running inside Windows Terminal's own job is fine.

### The gap cash fills

Coreutils for Windows deliberately withholds `kill`, `timeout`, `whoami`, `dir`,
`expand`, `more`, and warns about `date`, `echo`, `mkdir`, `cat`, `ls`, `rm`, `sort`,
`tee`, `uptime` — purely because those names collide with PowerShell aliases and cmd
builtins. **Under cash that conflict table is void**; cash owns name resolution.

**Delivered.** Every name on both halves of that table is a cash builtin, verified with
an empty `PATH`. `kill` is cash's own (D11, D21, D22); the rest come from D48's bundle,
with `timeout` and `uptime` wired in because upstream's bundle had never included them —
`timeout` returning POSIX's 124 on expiry, `uptime` reporting real Windows uptime.

This was the concrete claim in "the gap cash fills", so it is worth stating that it now
holds rather than leaving it as an argument. Their other
documented limitations — no `/dev/null`, CRLF breaking byte-oriented tools, utilities
emitting backslash-separated paths that poison downstream pipes — are all fixable at
the shell layer, which is precisely where cash sits.

---

## 3. Decisions

In [decisions.md](decisions.md), by topic and under the same numbers, with every
decision made since (moved 2026-10-11).

## 4. Deliberate divergences from bash

D2 promises unmodified POSIX scripts run as-is. These are the places cash knowingly
differs. **The list is meant to stay short** — every entry is a future bug report from
someone who expected bash, so additions need to earn their place.

| # | Divergence | Why | Ref |
|---|---|---|---|
| 1 | `pwd` prints `C:/src`, not `/c/src` | Rendered paths become native exe arguments | D3 |
| 2 | `$PATH` renders Unix-style while other paths render Windows-style | Colon separation is incompatible with `C:` | D5 |
| 3 | Globbing and `find -name` are case-insensitive by default | Case-sensitive matching can only produce false negatives on a case-insensitive volume | D16 |
| 4 | `\r\n` terminates a line in `$(...)`, `read`, `mapfile`, here-docs | Python/.NET/cmd emit CRLF and cannot be fixed at source | D20 |
| 5 | Redirections above fd 2 error for native exes | Windows exes have no POSIX fd ABI | D26 |
| 6 | Environment lookup is case-insensitive | Windows supplies `Path`, not `PATH` | D31 |
| 7 | `test -x` requires ACL **and** extension/shebang | Default ACLs make every file execute-granted | D23 |
| 8 | `chmod -x` warns and does nothing | Revoking execute needs Deny ACEs | D34 |
| 9 | `kill -STOP` suspends threads, not a real `SIGSTOP` | Windows has no `SIGSTOP` for arbitrary exes | D19 |
| 10 | A `>(...)` runs in the shell, which waits for it before it exits; handed as a path it is a named pipe to a builtin, and to a program or function a temp file read as it is written, a moment behind | Windows has no `/dev/fd` to hand a pipe on, and no `fork`; a named pipe cannot be created or truncated | D17 |
| 11 | `[ -s file ]` is false for App Execution Aliases | They are genuinely 0 bytes | D46 |
| 12 | Elevated and `detach`ed processes, and GUI applications, survive cash; other processes it started do not | Integrity boundary; breakaway flag; an editor should outlive the shell (`cashctl gui-apps close` to reap them) | D6, D42, D45 |
| 13 | A bundled builtin other than `rm` (`rmdir`, for one) cannot delete the shell's current directory; `rm` moves out of it first | It re-enters the binary as a child inheriting that cwd, and Windows refuses to delete a process's own cwd | D48 |
| 14 | `$!` for a background job that starts no program is a number of cash's own (4n + 1) that no process has; `kill` and `wait` take it to mean the job | bash forks and reports the subshell's pid; cash runs the job on a thread of its own process, so there is no process to name | D70 |
| 15 | `kill 0` signals the trees cash spawned, not a process group | Windows has no process group that excludes the terminal; the console-wide alternative would kill it | D22 |
| 16 | `kill -1` is refused | "every process I may signal" on Windows reaches far past anything a script could mean | D22 |
| 17 | `sh`, `bash` and `cash` are cash, ahead of `PATH`, however the command is started: by name, through `exec`, `command`, `xargs`, `find -exec`, `nohup`, `env` and `timeout` | Otherwise `bash` is the WSL launcher and a script continues under Linux; `cash` is often not on `PATH` at all, since a terminal starts it by full path | D7 |
| 18 | `chmod` changes only the read-only attribute, from the owner's write bit | Windows has no execute or read bit outside ACLs and no group or other bits per file; where something is lost (`-x`, `-r`, setuid, setgid, sticky) it warns and returns 0, and the rest (`+x`, `+r`, `go-w`, a numeric mode's other bits) is silent (the user, 2026-10-02 and 2026-10-03) | D23, D34 |
| 19 | `which` reports builtins; `stat` is cash's own, not uutils' | `which` must agree with the shell; uutils' `stat` is Unix-only, so cash carries one that shows a Windows file as Git Bash's does | D8, D48 |
| 20 | `id`, `$UID` and `$EUID` report the account's RID, not a uid, and 0 in an elevated shell | Windows identifies a user by SID; the RID is its last component and the nearest true equivalent. Elevated, all three are 0, so `[ "$EUID" -eq 0 ]` and `[ "$(id -u)" -eq 0 ]` agree on "running as Administrator" | D48 |
| 21 | `$SHELL` names cash, replacing whatever launched it | `make`, `npm run` and editors read it to decide what to launch | D5 |
| 22 | A program on `PATH` whose reader went away ends as it chooses (Python: `BrokenPipeError`, status 1), not with 141 | Windows has no `SIGPIPE`; cash's builtins and bundled tools end with 141 in silence, as Bash's do | D71 |
| 23 | `disown` forgets a job but does not make it outlive cash | A process cannot leave a Windows job object once assigned; `detach` starts one outside it | D6, D45 |
| 24 | `ls` colours when stdout is a terminal | Linux gets this from an alias in a system rc file; Windows has none, and an alias is the one spelling a builtin has no path behind | D48 |
| 25 | `uname -s` is `Windows_NT`, `$OSTYPE` is `windows` | cash is native Win32, not MSYS or Cygwin; scripts testing only `MINGW*|MSYS*` will miss their Windows branch | D48 |
| 26 | Bundled `sed` and `awk` keep CRLF lines CRLF and match them without the CR | Windows files stay intact and `$` works on them; a program naming `\r`, or `CASH_EOL=lf`, gets Linux behaviour | D49 |
| 27 | Arithmetic never executes `$(...)` found in an array subscript inside a variable's value | Bash runs it (`read n; echo $((n+1))` with input `a[$(cmd)]`), a well-known code-injection hole; Cash reports an error for indexed arrays and uses the text as a literal key for associative ones | — |
| 28 | `fuser DIR` and `lsof DIR` report holders of the files below the directory, not processes using it as their working directory; lsof's DEVICE and NODE are `-`, and its FD is 0 to 2 for the standard handles and a handle value, not a descriptor, for the rest | Windows exposes no per-process descriptor or working-directory information through a documented API | D50 |
| 29 | `ss` prints Recv-Q/Send-Q as `0` unless an elevated `ss -i` switched statistics on for the connection, `fd=-` for processes, every UDP socket as `UNCONN` and a dual-mode socket as two lines; `ss -i` shows only the SYN's MSS unelevated; `ss -K` closes only IPv4 connections, and only elevated | Windows' socket tables carry no queue sizes, descriptor numbers, UDP peers or dual-mode flag; per-connection statistics are collected only once an elevated process switches them on, and `SetTcpEntry` is IPv4-only and needs elevation | D51 |
| 30 | At the interactive prompt, an unquoted word starting `C:\` keeps its backslashes (`shopt winpaths`, off in scripts) | Pasted Windows paths are otherwise mangled to `C:Usersme` | D53 |
| 31 | `TERM` terminates a console program at once, and gives a program with a window five seconds after `WM_CLOSE` | A console control event cannot be aimed at one process that leads no group | D21 |
| 32 | `pgrep`, `pkill`, `pidof` and `killall` match names without case and with `.exe` optional; the kill family never signals the shell, system images or service accounts' processes | Windows image names are case-insensitive, and killing `csrss.exe` is a blue screen | D54 |
| 33 | `rev` keeps a CRLF line's `\r` at the end; `getopt -s csh/tcsh` is refused; `clear`/`reset` ignore terminfo and refuse non-VT terminal types | D20's line endings; cash is not a C shell; ConPTY speaks VT | D55 |
| 34 | `bc` is POSIX bc: GNU bc's language extensions are errors, each named, except a one-line `define` and a comparison as an expression, valued 1 or 0 (2026-10-17); after an error, `bc` exits 1 where GNU bc exits 0 | Decided scope (Q4); a named error beats a guessed extension, and a failed calculation should fail | D56 |
| 35 | `ping` is iputils' `ping`, not `ping.exe`; a reply slower than the interval counts as lost, and IPv6 replies show no `ttl=` | Scripts use Linux's `-c`; the Windows ICMP API has one echo in flight and no IPv6 hop limit | D57 |
| 36 | `which ls` prints `C:/…/cash.exe/ls`, a path no file is at, which cash runs as `ls` | A builtin has no file, and scripts run what `which` prints | D58 |
| 37 | A subscript that `unset`, `read`, `printf -v`, `declare` or `[[ -v ]]` expands a second time never runs a command substitution: `unset "a[$key]"` with `key='$(cmd)'` is an error | Bash runs `cmd`, its best-known array injection; `$i` and `$((…))` still expand as in Bash (as 27 does for arithmetic) | — |
| 38 | A `CHLD` trap runs once per child process cash starts and reaps; a bundled tool (`ls`, `cat`) and a command substitution run inside cash and raise none | Windows has no `SIGCHLD`; cash emulates it from the children it waits for, and those have no process | D64 |
| 39 | `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/fd/N` work in a redirection and in `source`, and share the descriptor: `> /dev/stdout` never truncates the file standard output is writing, and a read from `/dev/fd/3` goes on where 3 stands. A bundled tool opens them as well (`cat /dev/stdin`) and the file tests know them; a program on `PATH` does not | Windows has no `/dev` to open a second time, and an argument reaches a command as written | D7, D4 |
| 40 | Tab completes a typed `*` and `?` as globs (`cat **/nee`, `cat *.tx`); every other character, `[`, `(` and `!` included, is completed as typed, as in Bash | No Windows file name can hold `*` or `?`, so they can only mean a glob; Bash completes them literally and finds nothing (the user, 2026-10-03; LANG-16) | D40 |
| 41 | With `FUNCNEST` unset, functions nest at most 500 deep: the 501st call fails as `FUNCNEST=500` would, with Bash's message, and abandons its top-level command | Bash has no limit and its stack decides: Git Bash 5.3 crashes (139) between 600 and 800; cash's own stack holds about 2,000, and 500 keeps it far from that | — |
| 42 | The children's CPU time `time` and `times` report counts a child that is still running, such as a background job running through the `time`d command | Windows has no `getrusage(RUSAGE_CHILDREN)`; the session job's accounting counts every process it has held, cash's own share taken out (the user, 2026-10-02; EXE-11) | D6 |
| 43 | With `HISTSIZE` and `HISTFILESIZE` unset, history is not cut: every line is kept, in memory and in `~/.cash_history`, until one of them is set | Bash sets both to 500 when they are unset; following it would cut an existing long `~/.cash_history` to 500 entries at the next start (the user, 2026-10-03) | — |
| 44 | `\v` and `\V` in a prompt give cash's version (`1.3`, `1.3.13`), as `\s` gives `cash`, while `$BASH_VERSION` says `5.3.15(1)-release` | A prompt in cash is about cash; Bash's give Bash's version (the user, 2026-10-03) | — |
| 45 | In a folder whose path is longer than 258 characters, a program starts in the folder's 8.3 short name, and sees that as its working directory; with no short name that fits, it fails with status 126 | Windows starts no process in a longer one; Git Bash's native programs fail there (the user, 2026-10-04) | D29 |
| 46 | At the interactive prompt, the words after a program in System32 or SysWOW64 are not globbed (`shopt winglob`, off in scripts): `net user NAME * /add` reaches `net` as typed | cmd never globs, so every program Windows ships reads `*` itself or means something else by it; bash's globbing handed `net` the folder's file names (the user, 2026-10-11) | D81 |

`select` was missing outright until recently: it was a reserved word with no grammar
rule, so `select x in a b; do …; done` was a syntax error that took the whole file with
it. It is implemented now — menu and `PS3` on standard error, `REPLY` holding the raw
answer, an out-of-range or non-numeric choice leaving the variable empty — and matches
bash byte for byte.

**One near-miss, recorded because it was nearly an entry of its own.** The bundled
utilities are portable Rust, so the few that *construct* an absolute path spelled it
natively: `mktemp -d` printed `C:\Users\me\AppData\Local\Temp\tmp.AbCdEf`. A script then
did `d=$(mktemp -d)` and a plain `find "$d" | xargs grep` silently produced
`C:Usersme...`, because `xargs` reads `\` as an escape — a wrong answer rather than an
error, which is the failure mode this spec rejects everywhere else. Rather than document
it, D48 renders those utilities' output through D3 on the way out. See D48.

---

## 5. Architecture

```
Windows Terminal
      |
    ConPTY
      |
   cash.exe                    (crates/cash: the binary, `cash doctor`)
      |
      +-- cash-shell              (start-up, options, the bundled tools' dispatch)
      +-- cash-parser             (bash grammar; brush's, absorbed, D9)
      +-- cash-core               (expansion, control flow, traps; brush's, absorbed)
      +-- cash-builtins           (bash's builtins, and cash's: winpath detach
      |                            elevate start, D45)
      +-- cash-interactive        (line editing, history, completion, D18)
      +-- cash-coreutils-builtins (uutils coreutils as builtins, D48)
      +-- cash-sed, cash-awk, cash-bc (sed from uutils, awk and bc from posixutils-rs)
      |
      +-- cash-win32              <-- the Win32 layer
      |     path model                                   (D3, D10, D29)
      |     command resolution + PATHEXT dispatch        (D8)
      |     child env construction, PATH translation     (D5)
      |     CreateProcessW + job objects                 (D6, D36)
      |     job control, signals, suspend                (D11, D13, D19, D21, D22)
      |     redirection: /dev/*, fd table -> HANDLEs     (D7, D26)
      |
      +-- native Windows processes
            git.exe, grep and diff from Git or MS Coreutils, terraform.exe, Scoop shims
```

(`cash-test-harness` runs the YAML compatibility cases, D43; it is not in the binary.)

---

## 6. The `cash-core` seam analysis

`cash-core` is genuinely built for embedding: `Shell`, `ShellBuilder`, a
`ShellExtensions` trait re-exported at the crate root, custom builtin registration via
`Shell::builder().builtin(name, ...)`, and an `ErrorFormatter` convention the maintainer
is actively extending.

But cash needs seams at four points, and only one of them exists or is planned:

| # | Seam cash needs | Status upstream |
|---|---|---|
| 1 | Command **name resolution** (PATHEXT, `.cmd` / `.ps1` dispatch) | No seam. Not proposed. |
| 2 | Child **environment construction** (PATH translation, D5) | No seam. Not proposed. |
| 3 | **Process creation** (raw `CreateProcessW`, job object assignment) | Proposed in [#1377](https://github.com/reubeno/brush/issues/1377) as `ExternalCommandSpawner` |
| 4 | **Path rendering / acceptance** throughout builtins and redirection | No seam; threaded through core. |

Two specifics that matter:

- **#1377's seam sits in the wrong place for us.** It hands the spawner a fully composed
  `std::process::Command` *after* name resolution and environment setup — i.e. after
  seams 1 and 2, which is exactly where cash's differentiator lives. The maintainer
  notes an earlier draft put the seam at `CommandExecutor` (before name resolution) and
  set it aside as too hard. Worth engaging on now, while it is still being designed.
- **`std::process::Command` is too weak for job-object-safe spawning.** Assigning a child
  to a nested job after `spawn()` leaves a race window where a fast-forking grandchild
  escapes. The clean fix is `CREATE_SUSPENDED` → assign to job → resume, but
  `std::process::Child` does not expose the thread handle. cash needs raw
  `CreateProcessW`. Mitigating factor: if cash itself is in the session job, children
  inherit it automatically, so the *session-level* guarantee (D6) holds regardless. Only
  *per-job* nesting races. *Since closed:* the thread handle is not needed, because
  `NtResumeProcess` resumes a process by the process handle `Child` does expose, and
  tokio passes a `Command`'s `CREATE_SUSPENDED` through; every program is contained
  before it runs (D6).

Seam 4 drove D10. Seams 1–3 drove D9.

**Conclusion:** embedding gets the language for free but not a clean *public* seam for
the Win32 layer, which is why D9 is a soft fork rather than a library dependency.

### 6.1 M0 reconnaissance — the `sys` layer (amends the above)

Reading the actual source changes the picture in cash's favour. There **is** a clean
platform seam; it is simply internal rather than public, which is fine for a fork.

`brush-core/src/sys/` splits by platform — `unix.rs`, `windows.rs`, `wasm/`, and a shared
`stubs/`. Each platform module is a short manifest naming which subsystems are real and
which fall back to stubs. `windows.rs` is 20 lines, and it is the single most useful file
in the repository for this project:

| Subsystem | Windows today | cash decisions that need it |
|---|---|---|
| `env` | **real**, 184 lines | D5, D31 |
| `fs` | **real**, 395 lines | D3, D29, D33 |
| `users`, `network` | **real** | — |
| `process` | shared `tokio_process` | D6, D36 — *the one true replacement* |
| `commands` | **stub** | D8, D32 |
| `signal` | **stub** — `Signal` is an *empty enum* | D13, D19, D21, D22 |
| `terminal` | **stub** | ConPTY, D39 |
| `fd` | **stub** | D26 |
| `input`, `poll`, `async_pipe` | **stub** | D18 |
| `resource` | **stub** | not needed |

For comparison, `unix.rs` implements all twelve; Unix pulls in `nix`, `libc` and
`command-fds`, while Windows pulls in only `check_elevation` and `whoami`. The Windows
port is genuinely minimal, exactly as "preview" implies.

**Why this materially improves D9.** The diff surface is far less invasive than assumed:

- **Most of cash's work is *additive*** — writing `sys/windows/{signal,terminal,commands,
  fd,poll,input}.rs`, files that do not exist yet. A new file cannot conflict on rebase.
  Signals in particular are pure greenfield: `Signal::from_str` currently always returns
  `InvalidSignal`, so `trap SIGINT` and `kill -TERM` simply do not work on Windows today.
- **Only `process` is a true replacement**, because `tokio_process` is shared with Unix.
  That is precisely what upstream #1377 is designing a seam for — so the one invasive
  change is the one already being solved in the open.

**On upstreaming — withdrawn.** This section originally argued that "implement the
Windows half of brush's existing `sys` abstraction" was close to the most upstreamable
contribution possible, and that D9 should be revisited. That was reconsidered and
declined: D9 is now a hard fork and nothing goes upstream. The observation that the work
is *additive* still holds and still matters — it keeps the fork tractable — but it is no
longer an argument about contribution.

---

## 7. Remaining open questions

All of the original Q1–Q16 are resolved into decisions. What remains is genuinely small,
and most of it needs a **test** rather than an argument:

**Needs testing, not deciding:**

- OSC 9;9 path spelling that Windows Terminal accepts (D39)
- Whether `FILE_SHARE_DELETE` on the temp file a seeking consumer of `<(...)` gets survives real tools'
  sharing modes (D17)
- Whether packaged apps launched via an alias land in cash's job object (D46)
- Which non-Go tools handle `CTRL_C_EVENT` but ignore `CTRL_BREAK_EVENT` (D13)

**Needs deciding, but not yet:**

- Atuin-compatible history backend (D44)
- Auto-quote composition with already-typed quotes (D40)

---

## 8. Milestones

M0 as originally framed — "prove `cash-core` is embeddable as a library" — was answered
by §6 before any code was written: **not cleanly**, which is what produced D9. The
milestone is re-scoped accordingly.

**M0 — fork bootstrap. ✅ Complete.** Vendor brush; get it building on Windows and its
differential suite running on Linux CI (D43). Establishes the baseline that D9's diff
surface is measured against. No cash semantics yet.

| Item | Result |
|---|---|
| Repo | initialised; `.gitattributes` enforces LF, excludes CRLF/BOM test fixtures |
| Vendoring | `git subtree` at `crates`, upstream pinned at `737dd57`, `subtree pull` path intact |
| Windows build | clean, 2m30s, **zero warnings** (workspace sets `warnings = "deny"`) |
| Smoke | `brush.exe -c` runs; reports `BASH_VERSION` 5.2.37(1)-release |
| CI | Linux conformance via `cargo xtask test integration`; Windows build + smoke; fmt/lint |
| Baseline | §9, measured rather than assumed |
| Bonus | §9.1 upstream parser bug found |

**M1 — the two headline behaviours. 🔶 Largely complete.** Replace the spawner with raw
`CreateProcessW` plus per-job nested job objects (D6), and implement D13's Ctrl-C
escalation. Proves the thing Windows does better than Linux, without breaking
`terraform apply`.

Delivered as `crates/cash-win32`, kept as its own crate so the Windows semantics and the
shell language stay separable. (It was originally outside a `vendor/` directory to keep
that tree pristine; D9's absorption made the boundary unnecessary, but the separation of
concerns was worth keeping.)

| Module | Decisions | State |
|---|---|---|
| `job` | D6, D45 | job objects, `process_ids`, breakaway opt-in |
| `spawn` | D6, D13 | `CREATE_SUSPENDED` → assign → resume; `wait`/`try_wait` |
| `session` | D6, D41 | session job installed before anything runs |
| `console` | D13, D19, D41 | escalation state machine, `CTRL_BREAK_EVENT`, suspend/resume |
| `path` | D3, D7, D29 | accept every spelling, render `C:/`, `\\?\` + lexical `..` |
| `env` | D5, D31 | PATH translated at the boundary; case-insensitive lookup |
| `text` | D7, D20, D41 | CRLF as terminator, CRLF script source, BOM stripping |
| `stdio` | D3, D48 | capture a bundled utility's stdout and render its paths |
| `exit` | D15 | truncation plus NTSTATUS → `128 + n` |
| `resolve` | D8, D46 | PATHEXT dispatch, extension before read, real on-disk casing |
| `cmd` | D32 | CRT quoting, caret escaping, `is_safe_for_cmd` |
| `process` | D42, D48 | `is_pid_alive`, `cpu_time`, process listing for `ps` |
| `pathsearch` (core) | D7 | `sh`, `bash` and `cash` resolve to cash, ahead of `PATH` |
| `sys::windows::fs` (core) | D5 | `PATH` split on either separator; here-docs backed by a temp file |
| `jobreg` | D6 | per-spawn nested job registry, tree kill, sweep |

Plus `crates/cash`: the binary, and `cash doctor` (D35).

**72 tests, zero warnings** (the workspace sets `warnings = "deny"`).

**`cash doctor` works and is useful today.** Run against the machine this was designed
on, it independently rediscovered everything §9 found by hand — BusyBox applets
masquerading as `sed`, `awk` and `grep` through Scoop shims, and DOS `find`/`sort`/`more`
winning from System32. It also surfaced something the manual survey missed: **the
userland depends on which shell launched cash.** Under PowerShell's `PATH` the machine
reports fourteen warnings; under Git Bash's, which puts Git's MSYS tools first, it
reports one. Same machine, same binary.

**D6 verified end to end.** `cash(outer) → cash(inner) → ping`, then `Stop-Process
-Force` on the outer alone — `TerminateProcess`, so no cleanup runs, no signal is sent
and nothing can cooperate. Both descendants died. On Linux the same test leaves the
descendants alive, reparented to init.

`cash.exe` runs, reports `BASH_VERSION 5.2.37(1)-release`, and confirms `session job
installed, utf8 console on, nested in another job yes` — nesting inside Windows
Terminal's own job behaving exactly as D6 predicted.

Still outstanding for M1:

- Wiring the Win32 layer into brush's `sys/windows/*`, which is the invasive part and
  the first real entry in D9's diff table
- D37's Starship prompt test, which depends on that wiring
- ConPTY (`sys/windows/terminal.rs`) and the fd table (D26)
- D45's builtins: `winpath`, `detach`, `elevate`, `start` — the path conversions behind
  `winpath` already exist in `cash_win32::path`

**M2 — a real script runs. ✅ Complete.** A Terraform-wrapper-shaped script, written the
way such scripts are actually written on Linux — no Windows accommodations, no
cash-specific spellings — runs unmodified and **its output matches real bash exactly**.

It exercises `set -euo pipefail`, an EXIT trap doing cleanup, `mktemp`, command
substitution over a CRLF file, `$(pwd)` composed into an argument, `case`, functions with
locals, arrays, parameter expansion with defaults, arithmetic, a here-document, a `read`
loop, process substitution, and a status checked against a conditional. It lives in
`tests/corpus/` and is run by `crates/cash/tests/corpus.rs`, which also diffs it against
the reference bash — a genuine cross-check, because the script deliberately avoids every
§4 divergence.

**One divergence it surfaced**, now §4 #13: the near-universal cleanup idiom

```bash
d=$(mktemp -d); trap 'rm -rf "$d"' EXIT; cd "$d"
```

failed at the `rm`. A bundled builtin (D48) re-enters the binary as a child process and
inherits the shell's working directory, so this is a process deleting its own current
directory, which Windows refuses. bash-on-Windows succeeds because Cygwin emulates POSIX
unlink semantics.

Running the builtin in-process would fix it, since cash's own process never sits in the
shell's folder (D10); it is not done because re-entry is exactly what makes redirections
and pipes work for uutils. Since 2026-10-17 `rm`, cash's own code, moves its process to
the volume's root before it removes a folder that holds it, and the idiom works; a
relative operand after that names nothing, as on Linux once the working directory is
gone. The other bundled tools still cannot (`rmdir "$d"` from inside `$d`). The form
`trap 'cd /; rm -rf "$d"' EXIT` is good practice anyway, and is asserted by a test so it
stays working.

**Acceptance corpus.** M2 generalises into the spec's executable form: a collection of
real scripts that must pass, plus a test per row of §4. Worth starting early — it converts
prose decisions into tests and reveals which open questions actually matter in practice.

---

## 9. M0 baseline — measured, not assumed

Environment: Windows 11 26200, Rust 1.98.0 `stable-x86_64-pc-windows-msvc`, brush at
`737dd57`. Release build: **3m00s, zero warnings** — meaningful because the workspace
sets `warnings = "deny"`, so the Windows MSVC build is genuinely clean.

Measured by running probe scripts under `brush.exe`. This is the baseline D9's diff
surface is measured against.

| Behaviour | brush on Windows today | Gap for cash |
|---|---|---|
| `pwd` rendering | `C:\Users\thraa\...` — backslashes | **D3/D10** |
| `cd C:/Windows` | works | — |
| `cd /c/Windows` | **fails** | **D3** |
| `/tmp` | works | partial already |
| `$HOME`, `~` | `C:\Users\thraa` | D24 satisfied; rendering per D3 |
| `$PATH` form | `C:\...;C:\...` — semicolons | **D5** |
| `$PATH` / `$TEMP` populated, `$Path` / `$Temp` empty | names uppercased on import, lookup still case-sensitive | **D31** half done |
| `> /dev/null` | works | — |
| `[ -e /dev/null ]` | false | **D7** |
| `cmd.exe /c "exit 300"` → `$?` | `44` (low byte) | D15 base done; NTSTATUS map missing |
| `git`, `cargo` bare-name resolution | works, PATHEXT visible | D8 largely done |
| **glob `*.txt` vs `Upper.TXT`** | **already case-insensitive** | **D16 already satisfied** |
| `v=$(cat crlf.txt)` | 4 bytes — `val\r` retained | **D20** |
| `read` from CRLF file | 2 bytes — `b\r` retained | **D20** |
| `: > nul` | goes to the device, not a file | **D28** |
| `exec 3>&1` | rejected | **D26** |
| `cat <(echo x)` | fails | **D17** |
| `trap INT` | **rejected** | **D13/D14** greenfield |
| `kill -l` | prints HUP/INT/QUIT/… | cosmetic only — no real signals behind it |
| Script with UTF-8 BOM | `command not found: ﻿echo` | **D41**, exactly as predicted |

Two observations worth carrying forward:

- **D16 needs no work.** Case-insensitive globbing already falls out of the filesystem.
  The `shopt -u nocaseglob` half of D16 is untested.
- **`kill -l` lists signals that do not exist.** The builtin prints a table while
  `Signal` is an empty enum and `trap INT` is rejected. Cosmetic inconsistency upstream;
  cash's D11 replacement removes it.

### 9.1 Upstream bug found

`case` inside command substitution fails to parse:

```bash
x=$(case abc in a*) echo matched;; *) echo no;; esac)
```

- real bash (Git Bash 2.55): `matched`
- brush `737dd57`: `syntax error at line 1 col 33`

**Fixed here (D9), in two places.** Both the tokenizer and the word parser counted
parentheses without understanding `case`, so a pattern's unbalanced `)` — `a*)` — looked
like the end of the substitution.

- `brush-parser/src/tokenizer.rs`: `consume_nested_construct` now tracks `case`/`esac`
  depth and whether a pattern is expected (after `in`, or after `;;` / `;&` / `;;&`). A
  pattern's `)` is consumed without decrementing the nesting count; `(a*)` still balances
  normally because its opener is counted.
- `brush-parser/src/word.rs`: `unquoted_literal_text_piece` gained a `case_command()`
  rule so the same construct survives the word-level scan.

Both were needed: the quoted path (`"$(case ...)"`) goes through the word parser, the
unquoted path (`x=$(case ...)`) through the tokenizer. Fixing one left the other failing.

Verified against real bash across twelve cases including nested `case`, parenthesised
patterns, `esac` inside a quoted string, and `esacular` as a bare word. brush's own
parser suite still passes — 235 tests, including a new regression test.
