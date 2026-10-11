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

### D1 — Build on brush's code

cash is its own binary providing a Win32 semantics layer, built on `cash-core` and
`cash-parser` rather than starting from scratch. It inherits the bash compatibility test
suite.

The advertised Bash interface is 5.3.15: `$BASH_VERSION` is `5.3.15(1)-release`, claimed
once every Bash 5.3 item was implemented, verified or documented
(`research/bash-reference/bash-5.3-audit.md`), and 5.3.15 is the Git Bash build every
probe and ConPTY case is checked against. It was 5.2.37, with the 5.3 `${ command; }`
and `${| command; }` substitutions and `compgen -V` as single features, until then. The
focused source comparison and regressions are recorded in
`research/bash-reference/README.md`.

*How* it builds on them — library dependency versus fork — is D9, decided after the seam
analysis in §6. D1 asserts only that the language layer is not rewritten.

**Sizing, measured at M0.** The question "is brush worth it, or should cash be written
from scratch?" was re-examined against numbers rather than intuition:

| Crate | Lines |
|---|---|
| `cash-core` | 22,972 |
| `cash-parser` | 9,667 |
| `brush-builtins` | 7,636 |
| `brush-interactive` | 4,177 |
| **language layer** | **~40,300** |
| Differential test cases | **2,631** across 118 files |

Against an estimated 5–8k lines for cash's Win32 layer.

**The decisive evidence is §9's failure distribution.** Every probe failure — paths,
CRLF, `nul`, `exec 3>&1`, `trap INT`, BOM, process substitution — was in the *stubbed
Win32 layer*, which cash writes from scratch either way. Nothing failed in the language,
across functions, nested command substitution, `while read`, globbing, parameter
expansion, arithmetic, pipelines and redirections. The single language-level bug found is
§9.1's parser edge case.

So from-scratch buys nothing on the hard Windows parts — they are stubs regardless — and
costs the entire language plus the encoded edge-case knowledge in 2,631 differential
tests.

**The one honest case for from-scratch is dropping D2.** A purpose-built Windows shell
with its own syntax is a coherent project; it is simply a different one, and "runs my
Terraform scripts unmodified" goes with it.

**Accepted risk:** brush is one maintainer's project and Windows is explicitly preview.
Abandonment would leave cash owning a ~40k-line fork. Under D9 that is the situation
regardless — cash owns its copy and upstreams nothing — so abandonment costs only future
fixes, not the existing code. MIT licence, clean build, 235 parser tests that run here.

### D2 — Compatibility target: unmodified POSIX-shaped `.sh` scripts

Existing scripts run as-is. Windows-ness is the substrate, not the surface syntax.

Deliberate exceptions are enumerated in §4 — that list is meant to stay short.

### D3 — Path model: Windows first-class, Unix spellings accepted

**The canonical spelling is `C:/test/dir` — drive letter, forward slashes.** Never
backslashes on output.

- **Accept** `C:/src`, `/c/src`, `/tmp`, `/dev/null` as input wherever cash itself opens
  the path (D10): redirections, `cd`, `source`, `test` and `[`, and cash's own builtins. An
  argument to a program or a bundled tool (D48) reaches it as written (D4), and none of
  them has `/c` or `/tmp`; when the `C:/` spelling names something that exists, cash warns
  and gives it (`cat /c/Windows/win.ini`).
- **Render** drive-letter + forward slash, always: after `cd /c/src/infra`, `pwd` prints
  `C:/src/infra`. One canonical form; no modes, no provenance tracking.
- Rationale: rendered paths usually become arguments to native `.exe` files, where the
  wrong spelling is fatal rather than cosmetic. `terraform -chdir="$(pwd)/modules"`
  must work. Win32 file APIs accept `/` as a separator, so this costs nothing.
- **Known casualty:** `case $PWD in /c/*)` stops matching. Accepted.
- Internally: `\\?\`-prefixed UTF-16 (D29). UNC paths supported.

**One documented exception to the rendering rule: `$PATH` (D5).** It renders Unix-style,
because colon-separation is incompatible with `C:` drive letters. So `echo $PWD` gives
`C:/src` while `echo $PATH` gives `/c/tools:/c/Windows`. This asymmetry is deliberate and
is the only one.

#### Backslashes: accepted as input where the grammar allows, never rendered

`C:\test\dir` **cannot** work unquoted, and this is not a choice cash gets to make.
Backslash is bash's escape character, so `cd C:\test\dir` lexes to `C:testdir` before the
path layer ever sees it. Supporting it would mean breaking bash's quoting rules, which is
a direct hit on D2. (At the interactive prompt, D53's `winpaths` does support it, in a
narrow form; scripts keep this rule.)

Quoted, it works — and must, because you will paste paths from Explorer and from Windows
error messages, and native tools emit backslash paths in their output:

```bash
cd "C:\Users\thraa"          # works: \t and \U are not escapes inside double quotes
cd 'C:\Users\thraa'          # works: single quotes are literal
out=$(some-tool.exe --path)  # emits C:\foo\bar
cd "$out"                    # works: the string holds real backslashes
```

The rule: **the path layer accepts either separator in whatever string reaches it.**
Whether a backslash survives to that layer is the lexer's business and follows bash rules
exactly. Clean separation, no special cases in the grammar.

**A caveat on forward slashes.** A few old DOS-lineage tools parse a leading `/` as a
switch prefix and can misread `C:/foo` style arguments. Rare in the toolchain from §1
(Terraform, git, MS Coreutils are all fine), but it is why D4 matters: if a tool needs
backslashes, you spell them yourself — or use `winpath` (D45) — and cash passes the result
through untouched.

### D4 — Never rewrite arguments

No scanning of argv for things that look like paths. This is where MSYS2 went wrong and
why it needs `MSYS2_ARG_CONV_EXCL`. If a variable holds a string, the child gets that
string.

The deliberate escape hatch is `winpath` (D45). The one narrow exception is quoting for
`cmd.exe`, which is not semantic rewriting — see D32.

### D5 — `PATH` is the single exception, translated at the process boundary

`PATH` is the one variable cash has semantic knowledge of.

- **Rendered to scripts** Unix-style: colon-separated, `/c/...`. So
  `IFS=: read -ra dirs <<< "$PATH"` works, and `PATH=/foo:$PATH` works.
- **Held internally** as a typed list used for command lookup.
- **Converted to semicolon-separated Windows form** only in the environment block handed
  to a child process, so `terraform.exe` and `git.exe` get a `PATH` they understand.
- **No other variable is translated.** `GOPATH`, `PYTHONPATH`, `CLASSPATH` pass through
  verbatim. A known-list would be a maintenance surface and a source of silent surprise.

Interacts with D31: lookup is case-insensitive, so `$Path` and `$PATH` are the same
variable and both render Unix-style.

### D6 — Process lifetime: per-job nested job objects

Every pipeline / background job gets its own nested job object. Killing a job reaps its
whole tree. A session-level job with `KILL_ON_JOB_CLOSE` means nothing survives cash —
including cash being killed from Task Manager.

**Both halves implemented.** The session guarantee was wired first and proved by killing
cash with `TerminateProcess` while descendants ran. Per-job nesting came later, and the
gap it closed was measurable:

```
before kill %1 : ping=1 cmd=1
after  (before): ping=1 cmd=0    <- grandchild orphaned
after  (now)   : ping=0 cmd=0
```

**The spawn race is closed.** §6 records that assigning a child to a job *after*
`spawn()` leaves a window in which it can fork a grandchild that never joins, and that
`CREATE_SUSPENDED` → assign → resume closes it. The shell spawns through tokio, which was
first thought unable to start a process suspended, so the wiring at first assigned after
the spawn with that window open. tokio takes a `std` `Command`'s creation flags, so every
program is now created suspended, contained in its job, and only then resumed
(`cash_core::sys::tokio_process::spawn`): it cannot run an instruction, let alone start a
grandchild, outside the job.

**A finished command's job is released, not reaped.** When a command's root process
has exited, cash clears `KILL_ON_JOB_CLOSE` on its job before closing the handle, so
whatever it left running keeps running, as in bash. `code.cmd` starts the VS Code window
and exits; reaping its job when the next command ran closed the window as it opened.
`kill` of a live job still terminates its tree.

**GUI applications outlive cash.** On an orderly exit — `exit`, end of input, the
console window being closed, logoff or shutdown — cash terminates the console programs
in the session job and clears `KILL_ON_JOB_CLOSE` on it, leaving GUI-subsystem processes
and their descendants (VS Code's terminals and language servers) running, as PowerShell
does. `cashctl gui-apps close` turns this off for the session; `cashctl gui-apps` prints
the setting. A crash or a kill from Task Manager still reaps everything.

**Three documented exceptions to that guarantee.** They must be stated wherever the
guarantee is claimed:

| Exception | Why | Ref |
|---|---|---|
| Elevated processes | Cannot be assigned across an integrity boundary | D42 |
| `detach`ed processes | Requires `JOB_OBJECT_LIMIT_BREAKAWAY_OK` on the session job, which any child can then exploit | D45 |
| GUI applications, on an orderly exit | An editor opened from the shell should not close with it; `cashctl gui-apps close` restores the full guarantee | D6 |

Resolved sub-questions: `trap EXIT` ordering → D14; the `detach` builtin → D45.

### D7 — No modes. Compat behaviours are always on

Every compat behaviour is a *superset* that cannot break a Windows-native script —
accepting `/c/foo` costs nothing if you never type it. So: no `--posix` flag, no
shebang sniffing, no `set -o` switch.

Always on:

- `/dev/null`, `/dev/tty`, `/dev/stdin`, `/dev/stdout`, `/dev/stderr`, `/dev/fd/N`,
  `/dev/zero`, `/dev/random`, `/dev/urandom`
- CRLF tolerated in script parsing (D20 covers CRLF in *data*)
- UTF-8 BOM stripped from scripts (D41)
- Virtual `/usr/bin/env`, so `#!/usr/bin/env bash` resolves with no fake filesystem,
  and reads the line as GNU `env` does: `-S` splits it, then assignments, `-i`, `-u`
  and `-C` come before the command
- Unix path spellings accepted as input (D3)

**`sh` is Bash's POSIX mode (the user, 2026-10-04).** No compat behaviour is switched
off, but a cash started as `sh` (`sh -c …`, `exec sh`, `#!/bin/sh`, `#!/usr/bin/env sh`)
runs with `set -o posix`, every builtin still there, as Bash started as `sh` does, and
says `sh` for `$0`. A cash started as `bash` says `bash`. Windows gives a program no
`argv[0]` of its own, so the cash that starts it says the name in `CASH_ARGV0`, which the
started cash reads and removes; `exec -a NAME` reaches a cash the same way (EXE-12).
Another program cannot be given one, so `exec -a` and `exec -l` refuse it rather than
run it under its own name (the user, 2026-10-04).
`exec bash` ran Git's bash, found on `PATH`, until then.

**CRLF in script source is the one that earns its keep.** `core.autocrlf` is `true` by
default in Git for Windows, so *every* `.sh` in a repository checked out on this machine
has `\r\n` endings unless someone wrote a `.gitattributes`. Without this, `fi\r` is not
`fi` and `EOF\r` does not close a here-document, so the whole file fails to parse — and
because the parser only notices when it runs out of input, the reported position is the
end of the file rather than anywhere near the cause. Measured: cash's own second corpus
script failed at "line 78" for a `\r` on line 45.

Mechanism: `\r\n` is rewritten to `\n` as script source is read, streaming, for files,
`-c` strings and `eval` alike. A *lone* `\r` is left alone — that is data inside a string
literal, and `printf 'progress\r'` means it. This is a different rule from D20 with a
different mechanism, even though both come down to the same two bytes: D20 is about CRLF
in what a script reads, this is about CRLF in the script.

**`/dev/null` needs a carve-out from D29.** D28 makes reserved names ordinary files, and
D29's blanket `\\?\` prefix is the mechanism that achieves it — so `\\?\C:\...\NUL` would
open a *file* named `NUL`, not the null device. `/dev/null` must therefore resolve to the
device explicitly (`\\.\NUL`), bypassing D29's prefix rule for this specific mapping.

Consequence, and it is consistent rather than accidental: `> /dev/null` discards, while
`> NUL` creates a file called `NUL` (D28). A POSIX script means the former; only a
Windows-ism means the latter.

**`/dev/tty` is the console, which is two files.** A script whose input or output is a
pipe talks to the terminal by name, the usual form of a terminal query:

```
printf '\033[16t' > /dev/tty
IFS= read -rs -t 2 -d t reply < /dev/tty
```

A terminal is one file; a console's keys are `CONIN$` and its screen `CONOUT$`. So what
`/dev/tty` opens depends on what it is opened to do: the keys for `<`, the screen for
`>`, `>>` and `&>`. Opened for both (`<>`) it is the keys, and cannot be written to.
Without a console (a detached process, a service) the redirection fails with Bash's
words for a process without a controlling terminal, `No such device or address`.

**`/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/fd/N` are the shell's
descriptors.** `echo "message" > /dev/stderr` is `>&2` written out, and as common in
scripts. Opening one of these names gives the descriptor the shell has under that number
at that moment: the one a redirection or a pipe put there, not the one the process
started with.

```
warn() { echo "$*" > /dev/stderr; }
warn careful 2> log                  # goes to log
echo 'echo sourced' | . /dev/stdin
exec 9> out; echo nine > /dev/fd/9
```

The descriptor is duplicated, which is what Bash's manual says Bash does on a system
without these files; nothing is opened a second time. So `> /dev/stderr` does not empty a
file that standard error is going to. Git Bash, where the name is a file that gets opened
again, does: `( echo one >&2; echo two > /dev/stderr ) 2> log` leaves both lines here
and only `two` there. A name whose descriptor is not open fails in Bash's words,
`/dev/fd/9: No such file or directory`, and no file is looked for or made in its place.

All of these names are what they are only at `/dev`, as written or on whatever drive the
working directory is: `C:/src/dev/null` is a file. N is a number as a listing of
`/dev/fd` would write it, so `/dev/fd/03` is a file as well. With a separator after it,
`/dev/null/` names a folder, which no device is, and fails as in Bash. The names are
case-sensitive, as Bash's own redirections and file tests take them: Git Bash's MSYS
programs accept `/DEV/STDIN`, Bash itself does not.

**`/dev/zero`, `/dev/random` and `/dev/urandom` are endless input (2026-10-02)**, as in
Git Bash, and discard what is written to them (`cash_win32::endless`). `/dev/zero` is a
temp file a tebibyte long and all hole: sparse, so it takes no space, deleted by Windows
when it is closed, and read in whole blocks, which `dd if=/dev/zero bs=1M count=8` needs,
as `dd` counts each read as a block. The random ones are a pipe a thread keeps full of the
system's random bytes, a mebibyte at a time; a read of more than that can come back short.

They are recognised where cash opens a file by name itself: the word of a redirection and
the operand of `source`.

**The file tests answer for them as Git Bash does (2026-10-02).** `[ -e /dev/null ]` was
false, as was every test of every name: the tests asked the filesystem. Now a test on a
name is answered from what the name is, by the same rule as a redirection's, with Git Bash
5.3's answers, measured: `/dev/null` and `/dev/tty` are character devices that can be read
and written (`-e -c -r -w -O -G`); a descriptor's name is a link (`-L`, `-h`) to what is
open under the number, which is a pipe read or written as its end is (`-p` and `-r` or
`-w`), a file as the file is (`-f`, `-s`, `-r`, `-w`), or a device; a name whose
descriptor is not open is nothing, every test false.

**A bundled tool opens them too (2026-10-02).** An argument reaches a command as written
(D4), so `cat /dev/stdin`, `tee /dev/stderr` and `cp /dev/null f` failed with "The
system cannot find the path specified." cash translates no argument, as D28's revocation
requires; the tools cash bundles accept the names themselves. They are 85 uutils crates
with no one function in Rust that their opens go through, but every open in `cash.exe` is
a call to `CreateFileW` in `kernel32.dll`. So in a bundled tool's own process, and never
in the shell, cash points its imports of that and of the calls beside it at its own
functions (`cash_win32::devices`), which answer the names as a redirection does and pass
every other path on:

- `/dev/stdin`, `/dev/stdout`, `/dev/stderr` and `/dev/fd/0` to `2` are the tool's own
  standard streams, duplicated, not opened again; `/dev/fd/3` and up are no file, as a
  program has no descriptor above 2 (D26). `/dev/null` is the null device, `/dev/tty` the
  console, its keys to read and its screen to write.
- A pipe or a device is an empty file to a tool that asks (`cat` whether its input is a
  folder, `cp` what to copy, `dd` where its output stands): Windows answers "Incorrect
  function", and uutils stopped there even on `NUL`. `cp` from or to a name copies by
  reading and writing.
- A name is one where a redirection takes it: at `/dev` at the root, with or without a
  drive, which is also what the standard library makes of `/dev/stdin` before it opens
  it (`\\?\C:\dev\stdin`). `C:/src/dev/null` is a file.

A program on `PATH` still gets the name as written and fails on it.

**`ls` and `stat` show them as Git Bash does (2026-10-02).** cash's own, which run in
the shell, asked the filesystem: "No such file or directory" for `/dev/null`. A device
is a character special file, `crw-rw-rw-`, owned by the user, with Linux's numbers
(`1, 3` for `/dev/null`) where `ls -l` shows a size and in `stat`'s `%t`, `%T` and
"Device type". A descriptor's name is a link to `/proc/self/fd/N`, and `stat -L` says
what is open under the number: a fifo, a file, a device. One whose descriptor is not
open is no file. Every one of them, the shell's redirections and file tests, `ls`,
`stat` and the bundled tools, knows the names by one rule,
`cash_win32::devices::dev_name`.

### D8 — Command resolution is cash's own

```
1. builtin / function / alias
2. resolution cache
3. PATH search, honouring PATHEXT
4. dispatch by type, extension checked BEFORE any file read:
     .exe .com         -> CreateProcessW
     .cmd .bat         -> cmd.exe /d /s /c   (arguments escaped per D32)
     .ps1              -> pwsh.exe (fallback powershell.exe)
     extensionless     -> read first bytes; shebang -> named interpreter
```

**Extension before read is load-bearing, not stylistic.** App Execution Aliases are
0-byte reparse points (D46), so reading one yields nothing. Checking PATHEXT first means
`python.exe` dispatches as a native exe and the empty read never happens.

Scoop shims are not special-cased — they are ordinary executables on `PATH` (D25). cash
must never *require* Scoop.

### D9 — brush is absorbed, not vendored

brush is MIT and so is cash, so the code is simply **absorbed**: the crates live in
`crates/` as cash's own, renamed `cash-*`, and are edited directly. Nothing is
upstreamed.

**Amended twice.** It began as "soft fork, upstream opportunistically", became "hard
fork, fix everything here", and is now full absorption. Each step removed machinery that
had stopped paying for itself:

- Upstreaming was declined, which made "what did we change versus upstream?" a question
  nobody needed answered.
- That in turn made the `vendor/` boundary pure overhead. It forced a second workspace, a
  second `Cargo.lock`, and a cross-workspace path dependency
  (`../../../crates/cash-win32`) just to let the shell see its own platform layer.

So the diff-surface table that used to live here is gone. It existed to police divergence
from an upstream cash no longer tracks; keeping it would have meant maintaining a map of
a border that no longer exists.

**What was absorbed** — at upstream commit `737dd57`:

| Crate | Was |
|---|---|
| `cash-core` | `brush-core` |
| `cash-parser` | `brush-parser` |
| `cash-builtins` | `brush-builtins` |
| `cash-interactive` | `brush-interactive` |
| `cash-shell` | `brush-shell` (library only; `crates/cash` is the binary) |
| `cash-coreutils-builtins` | `brush-coreutils-builtins` |
| `cash-test-harness` | `brush-test-harness` |

Absorbed later, from other projects: `cash-sed` from uutils' `sed`, and `cash-awk` and
`cash-bc` from posixutils-rs.

**What was discarded:** the `brush` binstall alias crate, `brush-experimental-builtins`,
fuzzing, benchmarks, upstream documentation, CI, devcontainer and release tooling, and a
test that validated upstream's README.

**Attribution.** MIT requires the copyright notice to travel with the code. `LICENSE`
carries cash's MIT plus a derivation notice; `NOTICE` reproduces brush's copyright,
records the absorbed commit, and lists every modification made to the absorbed code.
In-source changes are marked `cash (Dnn)` against the decision that motivated them.

**Accepted cost.** Pulling a future upstream improvement is now a manual port rather than
a merge. That is the deliberate trade for owning a coherent codebase instead of a patched
copy of someone else's.

### D10 — Path rendering via two chokepoints, not a core rewrite

D3's "always renders `C:/...`" is achieved without diffuse changes:

1. **cash's own `cd` builtin** stores the Windows-canonical form into shell cwd state.
   `$PWD` then renders correctly for free, because core just echoes what `cd` stored.
2. **The file-open boundary** accepts Unix spellings (`/c/...`, `/tmp`, `/dev/null`) and
   normalizes to `\\?\`-prefixed UTF-16 (D29).

**Known gap:** paths the core computes itself — glob results, `realpath`, tilde
expansion — bypass both chokepoints. Under D9 these are patchable as they surface; each
is a small localized fix rather than an argument for rewriting path handling.

### D11 — cash owns job control entirely

brush's job manager is disabled. cash implements `jobs`, `fg`, `bg`, `kill`, suspend and
resume in Win32 terms, so there is exactly one notion of "a job" and it is the one backed
by a job object (D6). Avoids two bookkeeping systems drifting apart.

Cost accepted: reimplementing a fiddly subsystem that already works upstream and is
tested against bash. This is the largest single piece of owned code.

Consequence: **signal semantics become cash's problem** — D13 (Ctrl-C), D19 (suspend),
D21 and D22 (`kill`).

**Status.** `sys/windows/signal.rs` replaces the stub whose `Signal` was an empty enum,
so `trap` and `kill` work for the signals Windows can honestly deliver — `INT`, `TERM`,
`HUP`, `QUIT`, `KILL`, `STOP`, `TSTP`, `CONT` — each backed by a real Win32 mechanism.
Signals with no mechanism behind them (`USR1`, `PIPE`, `ALRM`) are *refused*
rather than accepted-and-ignored, because a trap that can never fire is the silent
failure D20 and D26 both reject. `CHLD` was on that list; its event is one cash sees, and
D64 now implements it.

`kill` is also now a builtin on Windows. It had been gated to Unix by a single `nix::`
reference for its default signal, which meant `kill` fell through to whatever external
`kill.exe` happened to be on `PATH`.

**Background job process tracking — fixed.** `$!` was empty and `kill %1` had nothing to
signal, because a background job is created as `JobTask::Internal(join_handle)`: a tokio
task running the whole and-or list, not a tracked process. The child's pid lived inside
that task and never reached the job.

Rather than restructure how jobs are built, the job now installs a sink in
`ExecutionParameters` before spawning the task, and the spawn site reports into it. The
job consults the sink when its own tasks yield no pid. Small, and it leaves the existing
`External` path untouched:

```
$!            a real pid
jobs -p       lists it
kill %1       reaps the job
kill -9 $!    kills the process
```

This was inherited rather than Windows-specific — it behaved the same on Linux — but D11
claims cash owns job control, so it was cash's to fix. A job that starts no program has
a number of its own instead (D70).

**`jobs` and `wait` as Bash 5.3 has them (2026-10-02).** Every difference
`open-issues.md` entry 8 listed, measured against Git Bash 5.3.15:

- A subshell, a command substitution and a stage of a pipeline see the parent's jobs as
  they are when it is made (`Shell::subshell`), and `jobs -r` and `-s` choose among them:
  `while [ -n "$(jobs -pr)" ]` ended never.
- `$!` is the last background job's pid, in a subshell too, and after the job has left
  the table; it had been the current job's.
- A finished job keeps its number until it is reported. A script no longer reaps it when
  the next job starts; only past 1,024 finished jobs do the oldest leave.
- A `wait PID` or `wait %N` reports the job and leaves it in the table, where `%N` and the
  pid find it again, `jobs` does not show it and `wait -n` does not return it; the next
  job, `jobs` or a plain `wait` takes it out, with its status kept for `wait PID`. Once
  out, `%N` names nothing: `wait: %1: no such job`, 127.
- A plain `wait` forgets every saved status but `$!`'s, and that one only when its job
  had ended before the wait and nothing had reported it.
- `wait -n -p VAR` unsets `VAR` when there is nothing to wait for, and POSIX mode shows a
  failed job as `Done(3)`.

### D12 — The name is cash

crates.io is clear. The npm [dthree/cash](https://github.com/dthree/cash) collision is a
different ecosystem and a dormant project. Accepted cost: "cash shell" will never be a
clean search term.

### D13 — Ctrl-C escalates; the job object is the backstop, not the first move

```
1st Ctrl-C  -> console control event to the foreground job's process group
2nd Ctrl-C  -> escalate
3rd Ctrl-C  -> TerminateJobObject
```

**The grace period is the user, not a timer.** Terraform's own interrupt handling is
already two-stage — the first interrupt finishes the *current* operation, which can
legitimately take minutes mid-`aws_rds_instance`. Any fixed timeout either guillotines a
valid apply or is long enough to feel broken. The user is never more than one keypress
from escalating.

This makes D13 and D14 one mechanism rather than two. D21's `kill -TERM` keeps a real
timer, because no user is present to press anything.

**Rationale for graceful-first:** Ctrl-C during `terraform apply` must let Terraform
catch SIGINT and release its state lock. Reaping the job object first leaves a lock on
remote state and a `force-unlock` to recover from. D6's kernel-enforced tree kill is the
guarantee that nothing *survives*, not the mechanism for routine interruption.

**Two documented Windows traps, both of which have caught other projects:**

- **`CTRL_C_EVENT` cannot be delivered to a specific process group.**
  `GenerateConsoleCtrlEvent` with a nonzero group id *succeeds* but the signal is never
  received. Only `CTRL_BREAK_EVENT` is deliverable to a group. cash must therefore use
  `CTRL_BREAK_EVENT` for targeted delivery. Go's runtime maps both events to
  `os.Interrupt`, so the Go toolchain in §1 — Terraform, `gh`, `kubectl` — handles this
  correctly.
  - OPEN: non-Go tools that handle `CTRL_C_EVENT` but ignore `CTRL_BREAK_EVENT`. Needs a
    survey against the real corpus, not reasoning.
- **`CREATE_NEW_PROCESS_GROUP` disables Ctrl-C handling in the child by default**, which
  is a well-documented source of the "Ctrl-C does nothing" bug. Group creation flags must
  be chosen with this in mind, and covered by a test.

**As built (ROADMAP item 11).** Measured before deciding: a child in the console's group
dies of the keyboard's Ctrl-C, a child in its own group does not; a Ctrl-Break aimed at a
group ends Python at once (no `KeyboardInterrupt`) but only makes `ping.exe` print its
statistics, and leaves a `cmd` tree running; a Ctrl-C aimed at a group never arrives.
Giving *foreground* programs their own group would therefore mean relaying every Ctrl-C
as a Ctrl-Break, a regression for REPLs, so:

- **Background jobs at the prompt lead a group of their own.** With job control on,
  every external a `&` job starts is created with `CREATE_NEW_PROCESS_GROUP`. The
  keyboard's Ctrl-C passes it by, as it does a Unix background job — before this, Ctrl-C
  at the prompt killed every background job. Scripts keep the console's group.
- **Foreground commands are untouched.** Ctrl-C reaches them directly, however often it
  is pressed; a REPL keeps its session. There is no escalation for them: a program that
  ignores Ctrl-C still needs `kill -9` from elsewhere.
- **`fg` relays.** While `fg` waits on a job whose processes lead their own groups, cash
  sends each a Ctrl-Break on Ctrl-C; a second Ctrl-C terminates the job's tree through
  its job object. Only here, where cash is the one delivering the interrupt, does the
  escalation apply.
- **Leaders are registered with a handle held open.** A Ctrl-Break is only ever aimed at
  a pid cash started as a group leader and still holds a handle to, so the pid cannot
  have been reused by a process that leads no group (which the event would treat as
  "every process on the console", D21).
- **An interactive cash takes Ctrl-C back** from a parent that ignored it
  (`SetConsoleCtrlHandler(NULL, FALSE)` at startup). Windows passes the ignore flag to
  children, so a shell started by a build tool or task runner that ignores Ctrl-C would
  otherwise run programs nothing can interrupt. Scripts keep what they inherit, as POSIX
  has a non-interactive shell keep signals ignored on entry.

**What Ctrl-C does to the shell itself.** Windows sends the event to every process on
the console, the shell among them, and ends a process that has no handler for it where
it stands. Until 2026-09-30 that was the whole of it: a script waiting for a program went
on with its next command once the program had died, and a shell busy with commands of its
own was ended with no trap run, the interactive one included. Now the shell acts on an
interrupt as Bash acts on SIGINT, in three places:

- **After a foreground program.** A script ends only if the program died of the Ctrl-C:
  it exited with `STATUS_CONTROL_C_EXIT`, as a program without a handler does and as
  `ping.exe`, Python and Go programs do, or it is a shell that says so with status 130.
  A program that took the interrupt in its stride (a REPL that stays, `terraform apply`
  finishing its step) leaves the script running, which is what the bullets above promise
  it. `$?` of a program Ctrl-C ended is 130, as for SIGINT; it was 137 (D15).
- **Between commands of its own.** A Ctrl-C that arrives while nothing else of the shell
  listens is kept, and acted on before the next command. A second one that arrives
  before the first was acted on is left to Windows: a shell stuck in a command that does
  not return can still be ended.
- **In `read`, `select` and `mapfile`, at a console,** where the key is read as a key
  (`cash_win32::conin`).

Acting on it is one thing everywhere: a trap on `INT` runs (after the program has ended,
when there was one) and the shell goes on; without a trap a script ends with status 130,
its `EXIT` trap run, and the interactive shell abandons the command line and returns to
the prompt. A command started with `&` is not the keyboard's to interrupt.

### D14 — `trap EXIT` runs to completion; a second Ctrl-C forces teardown

No timeout by default — cleanup is sacred, and a guillotined cleanup is worse than a
slow one (releasing a remote lock over a bad connection legitimately takes time). The
escape hatch is a second interrupt during trap execution, which forces immediate
teardown.

Requires a signal path that remains live *while traps are running*. That is a design
constraint on the interactive/signal layer, not an afterthought — and it is D18's named
replacement trigger.

### D15 — Exit codes truncate, but crashes map to bash's 128+n convention

- Normal exit codes: low byte, as bash does.
- NTSTATUS exception ranges: mapped the way bash reports fatal signals — access
  violation → `139`, the same value a segfault yields on Linux. Scripts testing for
  `139` work unchanged.
- Rationale: naive truncation alone is unsafe. `0xC0000100` truncates to `0` — a crash
  silently reported as success.
- Cost accepted: a hand-maintained NTSTATUS → signal-number mapping table.

### D16 — Globbing is case-insensitive by default, and `nocaseglob` is honoured

Divergence from bash, chosen deliberately. On a case-insensitive volume `main.tf` and
`main.TF` cannot coexist, so case-insensitive globbing can only remove false negatives —
it can never introduce ambiguity. `for f in *.sh` matching `Setup.SH` is almost certainly
what the script author meant.

Equivalent to bash's `nocaseglob`, **on by default**.

`shopt -u nocaseglob` genuinely turns it off and globbing becomes case-sensitive. It is
cheap to honour, and a script that explicitly asks for case-sensitive matching has made a
deliberate choice cash should not override.

### D17 — Process substitution: temp files first, pipes now

*As first decided (the pipes that replaced it are at the end):* `<(...)` and `>(...)`
materialise a temp file and pass its path. Works with every
program, including ones that seek or stat for a regular file.

Rejected alternative: named pipes (`\\.\pipe\cash-NNNN` passed as the filename). Elegant
and truly streaming, but breaks for any tool that seeks or reopens the path.

**Known limitation, accepted:** no streaming. A non-terminating producer —
`while read l; do ...; done < <(tail -f app.log)` — collects forever instead of
streaming, and will appear to hang. Document prominently; consider detecting and warning.

**Cleanup is `FILE_FLAG_DELETE_ON_CLOSE`.** The kernel deletes the file when the last
handle closes — including on `TerminateProcess`, since the kernel closes handles during
process teardown. Cleanup therefore cannot depend on cash running an exit path, which
matters because D6 tears down without unwinding.

Pleasing symmetry: the same kernel-enforced, cannot-be-escaped property that makes D6's
job objects better than Linux process groups also solves temp file cleanup.

**Implemented**, and the cleanup question resolved the other way. `FILE_FLAG_DELETE_ON_CLOSE`
turned out not to work here: the child opens the file by *path*, and a delete-pending
file cannot be opened afresh. So this uses D17's named fallback — a per-session temp
directory, swept on first use by checking whether the owning pid still exists.

Also: **no fd is installed on Windows**. Upstream puts the pipe on descriptor 63 and
passes `/dev/fd/63`; doing the same here would make `inject_fds` reject the whole
command, because D26 makes a descriptor above 2 a hard error for native executables — and
that would be cash's own bookkeeping failing, not something the script asked for.

`>(...)` is refused with a clear message rather than silently doing nothing: it needs the
subshell to run *after* the consuming command, which the temp-file model has no hook for.

```
cat <(echo hello)                      works
diff <(a) <(b)                         works, correct exit status
while read l; do ...; done < <(...)    works
echo x > >(cat)                        clear error
```

**Now: pipes, which stream (`cash_win32::pipe`).** A substitution that is a redirection
(`< <(cmd)`, `> >(cmd)`, `exec > >(tee log)`) is joined to the command by an anonymous
pipe. One handed to a command as a path is a named pipe,
`\\.\pipe\cash-procsub-<pid>-<n>`, which native programs open with the ordinary file
calls; a program that must stat or seek its file (`diff`, `cmp`) gets a temp file,
written before it starts. The substitution runs in a subshell on a thread of the shell,
where Bash's is a process of its own, so `>(...)` works and `tail -f` streams.

**The shell waits for its `>(...)` before it exits (2026-09-30).** Bash's substitution
outlives the shell and writes what it has left after the shell has gone. cash's ended
with the shell, unfinished: `echo x > >(sleep 1; cat)` printed nothing, neither did
`exec > >(tee log)` for what the script wrote last, and a substitution caught starting a
program left that program suspended for ever, with `cash.exe` locked. Once the shell has
gone, and with it every write end of a substitution's input, cash waits for each
`>(...)` still running; one handed a path no program opened is given the end of its input
first. One that never ends keeps cash from exiting, as a command that never ends does.
`<(...)` is not waited for: its output has nowhere to go once the shell has gone.

**A `>(...)` handed as a path is a pipe or a file, chosen per command (2026-09-30).** A
named pipe cannot be created, truncated or appended to, and a program opens a file it is
to write in one of those ways: `tee >(cmd)` failed with "The parameter is incorrect",
`tee -a` with "Access is denied". So:

- **A builtin gets the named pipe,** as it opens the path as a pipe may be opened
  (`cash_win32::pipe::open_output`, through which cash's own opens go) or not at all.
  Among the bundled tools, `tee`, `sort`, `uniq` and `shuf`, cash's own code since
  2026-10-10 (`crates/cash-uutils`), open their output through it, and get the pipe
  too.
- **Everything else gets a temp file**, which every program can create and truncate: a
  program on `PATH`, a bundled tool that is not patched, a builtin that runs another
  command with its arguments or writes a file it is named (`command`, `exec`, `eval`,
  `source`, `xargs`, `find`, `install`, ...: `Registration::substitution_pipes`), and a
  function, which may hand the path on to any of these. The substitution reads the file
  as it is written, a moment later, and gives its disk space back as it goes, so a program
  that writes for hours does not fill the disk. The file is in the per-session directory
  above, and deleted as the substitution's input ends.

**A `>(...)` ends with the command it was handed to.** In Bash its input ends when the
last copy of its descriptor is closed: when the command has ended, and whatever that
started with it. Here the command's parameters hold it, as do the copies of them that
what it runs gets, and a program it runs until the program has exited; the last one to
let go ends the input. `x=$(echo >(cat))` waited for ever, as a path no program opened
held the substitution open until the shell exited. The file of a `<(...)` for `diff` or
`cmp` is deleted then too; they were never deleted before, and the sweep takes the ones
earlier versions left.

**Known `cmd` quirk, accepted (W32-02, the user, 2026-10-04):** `cmd /c type <(echo x)`
sometimes prints `x` and then "The pipe has been ended." — 11 runs in 2000 on a busy
machine, none in 300 on a quieter one. `type` reports the pipe's end as an error when its
read is already waiting as the pipe closes, which is the program's timing, not cash's: a
pipe has no other end than closing it. Keeping the pipe open until `type` had read
everything (`FlushFileBuffers`) made it more frequent, 49 in 2000. Other programs (`cat`,
`grep`, `sort`, Git's tools) read the same pipe without a word.

### D18 — Reuse `brush-interactive` now, replace when it blocks

Line editing, history and highlighting for free. Consistent with D9's velocity-first
stance, and it is why Starship works on day one (D37).

**Trigger for replacement** (named now, so it is not re-litigated later): the first time
D13's escalation or D14's second-Ctrl-C cannot be implemented because the input loop owns
the console. At that point the interactive layer moves into D9's diff surface table, and
D37's Starship test becomes the regression baseline.

### D19 — Suspend via thread enumeration, documented APIs only

`Ctrl-Z` and `kill -STOP` enumerate the target's threads and `SuspendThread` each; resume
reverses it. This is what Process Explorer's Suspend does. No undocumented
`NtSuspendProcess`.

**One exception: the resume of a program cash starts.** Every program is created
suspended, so that it is in its job object (D6) before it runs, and resumed with
`NtResumeProcess`, an `ntdll` export that is undocumented but has been there since NT and
costs one call; enumerating every thread of the system for each command would cost more.
Its result is checked (2026-10-03): when it fails, the program's threads are resumed one
by one with `ResumeThread`, as above, and when that fails too the program is ended and the
command fails with the error. Unchecked, a program it failed for stayed suspended and the
shell waited for it for ever.

**Scope follows D22**: `kill -STOP %1` suspends the job's whole tree; `kill -STOP 1234`
suspends that process only. `Ctrl-Z` targets the foreground job, so it is tree-wide. `fg`,
`bg` and `kill -CONT %1` resume the whole tree, and `jobs` shows the job `Stopped` in
between. (Until 2026-09-29 `kill -STOP %1` suspended only the job's first process, so
`cmd /c ping …` went on pinging, and `jobs` still said `Running`.)

**Ctrl-Z is a key, not a signal.** Windows has no Ctrl-Z event: a console control handler
hears Ctrl-C, Ctrl-Break, close, logoff and shutdown, and nothing else. Ctrl-Z is a record
with the character 0x1A in the console's input queue, for whichever program reads it, and
programs that read the keyboard use it as the end of input (`sort`, `copy con`, Python's
prompt), as Unix uses Ctrl-D. So while the interactive shell waits for a foreground job,
it looks at the queue every 50 ms without reading it, and takes a Ctrl-Z that has waited
there unread for 200 ms: it removes that key alone (keys typed around it stay, in order,
for the prompt), suspends the job's trees, shows `^Z` and `[1]+ Stopped`, and `$?` is 148.
A program reading the keyboard takes the key within milliseconds and keeps it. Decided
with the user on 2026-09-29, after a ConPTY test showed Ctrl-Z during `ping.exe` did
nothing at all and the key vanished.

Only the interactive shell itself listens, with job control on: a subshell, a command
substitution or a background job has no job table from which a stopped job could be
resumed. Nor while a stage of the pipeline runs inside the shell (a `while read` loop, a
builtin): a stopped pipeline still waits for such a stage, and one reading from a
suspended program would wait for ever.

**Console state per job**, as zsh keeps each stopped job's terminal modes: Ctrl-Z saves the
console's input and output modes and code pages, and `fg` puts them back; the prompt in
between sets the console as it needs (D68). Unix sends a resumed program `SIGCONT` and a
full-screen one redraws on it; Windows tells it nothing. So a job that was reading keys
one at a time, as a full-screen program does, is handed a resize record at `fg`, on which
such programs redraw. Best effort.

Accepted costs: racy against thread creation during the sweep, and a process could in
principle resume itself. A program busy for longer than 200 ms between two reads of the
keyboard can be suspended where it would have read the Ctrl-Z itself, and a key that
arrives while the queue is rewritten comes before the ones put back. None of these
matters for the workloads in §1.

### D20 — `\r\n` is a line terminator wherever cash interprets line boundaries

Not "strip `\r` from data". The rule, in one sentence:

> Wherever cash itself decides where a line ends, `\r\n` terminates a line exactly as
> `\n` does.

That covers `$(...)` and backticks, `read`, `while read`, `mapfile` / `readarray`,
here-strings and here-documents. Bytes flowing through a pipe between two external
programs are **never** touched — `a.exe | b.exe` stays byte-transparent.

**Why this is necessary and not merely convenient.** Repo files are fixable at source
with `.gitattributes` (`* text eol=lf`) and should be. Other programs' stdout is not:

| Source | Emits | Fixable upstream? |
|---|---|---|
| Go tools — `terraform`, `gh`, `docker` | LF | n/a |
| Rust tools — MS Coreutils, `rg`, `fd` | LF | n/a |
| Python `print()` (text-mode stdout translates on Windows) | **CRLF** | No |
| .NET tools (`Environment.NewLine`) | **CRLF** | No |
| Classic Win32 — `ipconfig`, `reg`, `findstr`, `sc` | **CRLF** | No |
| `cmd.exe` builtins and `.bat` output | **CRLF** | No |

`version=$(python get_version.py)` yields `1.2.0\r`, which then fails `[ "$version" =
"1.2.0" ]` while *printing identically* — the `\r` just returns the cursor. Note that
assignment performs no word splitting, so `IFS` cannot address this.

Risk is near zero: `$(...)` is already not byte-transparent in bash (it strips trailing
newlines and drops NUL bytes), so nothing legitimate passes binary through it.

cash's own builtins emit LF unconditionally.

### D21 — `kill -TERM` escalates asynchronously; `kill -9` terminates immediately

`kill -TERM` asks the one target process to stop and **returns at once**, so the POSIX
idiom — `kill -TERM $pid` followed by the script's own `wait` or timeout loop — keeps
working. `HUP`, `QUIT` and `INT` sent with `kill` do the same.

The asking goes through the target's windows: `WM_CLOSE` to each of its visible,
unowned top-level windows, which is what its close button sends and what `taskkill`
without `/F` does, so an editor gets to offer to save. If it is still running after five
seconds, a background thread terminates it, through a handle opened when it was asked, so
a pid Windows has reused in the meantime cannot be hit. A program with no window — a
console program — has no per-process way to be asked, and is terminated at once.

A process cash started as a group leader — a background job's, at the prompt (D13) — is
asked with a console Ctrl-Break as well, which console programs handle as an interrupt
(Go's toolchain as `os.Interrupt`, Python as `SIGBREAK`), and gets the same five
seconds. Any other process is never sent a console control event:
`GenerateConsoleCtrlEvent` addresses a process *group*, and aimed at a pid that leads no
group the event reaches every process on the console — this is how `kill -TERM $pid`
came to kill the shell that ran it, and the terminal's other programs with it (fixed
after 0.9.0). Programs that ignore Ctrl-Break (`ping.exe`, a `cmd` tree) are terminated
when the grace ends.

`kill -9` is an immediate `TerminateJobObject` / `TerminateProcess`, no grace.

Consistent with D13's graceful-first stance, but without D13's ability to use the user as
the timer: the shell is not waiting on a foreground job here, so a real timer is required.

Cost accepted: the escalation lives in the shell. A script that sends `TERM` and exits
before the grace period ends leaves a program that ignored its `WM_CLOSE` running.

**`kill` with no signal sends `TERM`**, as Bash and POSIX do; it sent `KILL`, so a plain
`kill $pid` skipped the grace above. **A process a signal terminates exits with 128 plus
the signal's number** (143 for `TERM`, 137 for `KILL`, 130 for the Ctrl-C escalation of
D13), the status `wait` reports in Bash; Windows lets the terminating side choose it, and
cash chose 1.

**A signal the shell sends itself** (`kill -TERM $$`) is not delivered through Windows,
which ended cash at once, trap or no trap. The shell handles it, as Bash does: `KILL`
ends it; a trapped signal runs its trap, with `BASH_TRAPSIG` set, and the shell goes
on; untrapped, a script ends with 128 plus the signal's number, and an interactive
shell ignores `TERM`, abandons the line on `INT` as Ctrl-C does, and exits on `HUP`.
`QUIT` is ignored in all cases, as Bash ignores it. Found by the ConPTY harness (ROADMAP
item 15), where each shell runs in a console of its own, so it could be checked against
Git Bash 5.3.15.

### D22 — Job specs kill trees; bare PIDs kill one process

- `kill %1` → the job's entire tree, via its job object (D6).
- `kill 1234` → that process only, as on Unix.

The spelling tells you the scope, so a ported script using `kill $pid` gets exactly
POSIX semantics and nothing surprising. D19 follows the same rule for suspend.

Note this does not weaken D6: a child orphaned by `kill $pid` is still inside the job
object and still dies when the job or session is torn down.

**A job's pid names the process cash started, or none.** A script keeps `$!` and uses it
after the job has ended: `kill "$pid"` in a cleanup trap, `while kill -0 "$pid"`. Bash
calls `kill(2)` on the number and is told "No such process", because Linux hands pids out
in ascending order and comes back to one after millions of others. Windows hands one out
again within a second on a busy machine (measured 2026-09-30: a second after a job was
killed, `kill -0 $pid` succeeded 8 times in 360, on another program), and cash asked only
whether some process had the number. So `kill -0` could report an ended job as running,
and `kill` could end a program the script never started.

cash therefore keeps a handle on each process of a job, background or stopped
(`cash_win32::children`). Windows does not reuse a pid while a handle to its process is
open, so the pid stays the job's after the job has ended, as a zombie's does on Linux
until its parent reaps it, and `kill $pid` and `kill -0 $pid` answer "No such process",
status 1. Running processes are held while they run; of those that have ended, the most
recent 1024, the number of finished jobs `wait PID` remembers. Past that the pid is free
again and cash knows nothing of it, as Bash knows nothing of a pid the kernel has reused.
An ended process that is held costs the kernel about 15 KB, is not listed by `ps` or Task
Manager, and does not keep its executable open. Of a pid cash never started as a job's
(one read from `ps`, a foreground command's), `kill` does what the number says, as
`kill(2)` does.

Whether a process still runs is asked of the process, through a handle, and not of its
exit status: a process may exit with 259, which Windows also reports for one that has not
exited.

**`kill %1` signals every process of the job that is still running**, as Bash signals
the job's process group: both ends of `a | b &`, and `b` in `{ a; b; } &` once `a` has
ended. It signalled the first process only, by pid, so after `a` had ended it failed
(`entity not found`) or reached another program, and `b` ran on. A process that has
ended is passed over, as Bash passes it over (`jobs.c`: "avoid pid recycling problem").
So `kill %1` and `kill -0 %1`, for a job that is still listed and has nothing left
running, do nothing, say nothing and return 0, as in Bash 5.3. `kill -STOP %1`, `fg`,
`bg`, Ctrl-Z and the second Ctrl-C under `fg` act on the running processes only, for the
same reason.

`kill 0` and the sweep of finished commands' job objects ask each tree's root, which the
registry holds open, whether it still runs. `killall -w` holds each process open before
signalling it and waits for those; asked by pid, it waited for as long as any process had
the number. `pkill` and `killall` do not signal a process that started after they listed
the processes: the pid was the listed one's and has changed hands.

### D23 — `test -x` requires an execute ACL **and** a discriminator

```
test -x FILE  ==  ACL grants FILE_EXECUTE
                  AND ( extension in PATHEXT  OR  file begins with #! )
```

`chmod +x` adds the execute ACE if it is somehow missing — usually a no-op — and returns
0, so `chmod +x deploy.sh && ./deploy.sh` works. On volumes with no ACLs (FAT32, some
network shares) the discriminator alone decides. `chmod -x` is D34.

**Why not ACLs alone.** Default NTFS inheritance grants a file's owner Full Control,
which includes `FILE_EXECUTE`. Every file in your profile therefore carries the execute
right, so `[ -x README.md ]` would be true and `for f in *; do [ -x "$f" ] && ./"$f";
done` would try to run everything. ACLs are the real permission model but on Windows
they do not *discriminate* — practically nothing sets a meaningful execute ACE.

**Why not the heuristic alone.** It invents a permission model that isn't there, and
`chmod -x` becomes a silent lie.

Requiring both keeps `test -x` honest about permissions while retaining actual signal.

### D24 — `~/.bashrc` is read automatically; `~/.cashrc` overrides

`$HOME` is `%USERPROFILE%`. Maximum continuity, inheriting brush's "your .bashrc just
works" behaviour.

**Accepted risk:** a `.bashrc` written for Linux sets `PATH` entries that do not exist
here and aliases tools that are not installed, producing noise or breakage at every
startup. D30 is the mitigation.

A user with neither file gets a starter `~/.bashrc` from Scoop's install (D69).

### D25 — Scoop shims are executed, not resolved through

cash has **no Scoop knowledge**. A shim is just another executable found on `PATH`,
honouring D8's "cash must never require Scoop" and keeping Scoop a package source rather
than a coupling.

Cost accepted: an extra process in every job tree, and `jobs` shows the shim rather than
the underlying command. Revisit only if job-tree noise becomes a real problem in use.

Two readers of a shim's `.shim` file, both outside command execution and both working
without Scoop (`cash_win32::scoop`): `cash doctor` sees through a shim to a BusyBox applet
(D35), and Tab completion starts carapace itself rather than its shim (D63). What a command
line starts is unchanged.

### D26 — File descriptors above 2 work where cash controls both ends

- **cash builtins and cash-to-cash:** full fd table, `3>&1` and friends work.
- **Arbitrary native exes:** a redirection above 2 is a **documented error**, not a
  silent no-op. Windows exes have no POSIX fd ABI; failing loudly beats vanishing.

Consistent with D20's principle that invisible failure is the enemy.

**Which case errors.** Only a redirection above 2 written on the simple command that
spawns the process: `tool.exe 3>x`, `tool.exe 4>&3`, `tool.exe {fd}>x`. That is the script
explicitly targeting a descriptor the child cannot see, so it is refused. Closing one
(`tool.exe 3>&-`) asks for nothing the child lacks and is not refused, and neither does one
the command copies down into 0, 1 or 2 (2026-10-02): in `cat 3< f <&3` and in the swap
idiom `cmd 3>&1 1>&2 2>&3` fd 3 is a step on the way, and the program gets what it held
under the number it can see. Set again after the copy, it is on the command once more.

Descriptors the shell merely *holds* are not passed and do not stop the command:

```
exec 3>&1 1>log; tool.exe; echo done >&3         tool.exe runs; fd 3 stays with the shell
{ tool.exe; echo x >&3; } 3>x                    tool.exe runs; the builtin echo writes x
while read -u 3 l; do tool.exe; done 3<file      tool.exe runs each iteration
f 3>x                                            externals inside f run
```

Bash hands fd 3 to an external inside `{ cmd; } 3>x`, so there the enclosing redirection
is "on" `cmd`. cash deliberately does not count it: the redirection belongs to the group,
whose builtins do use it, and `while read -u 3 ...; done 3<file` would otherwise make
every native exe in the loop body fail. The child could not have seen fd 3 either way.

The bundled coreutils (`cat`, `wc`, ...) re-enter `cash.exe` as a process and follow the
native-exe rule: `cat f` after `exec 3>log` works, `cat f 3>x` is refused.

Rejected: passing fds through the MSVC CRT's `STARTUPINFO` reserved-field table. It
genuinely works for MSVC-built targets including CPython, but behaviour would then vary
by how the callee was compiled — Go and Rust binaries would still not see fd 3 — which
trades a clear error for an inconsistent one.

### D27 — `ln -s`: real symlink, junction for directories, error otherwise

```
directory + privileged    -> CreateSymbolicLinkW
directory + unprivileged  -> directory junction
file      + privileged    -> CreateSymbolicLinkW
file      + unprivileged  -> error, naming Developer Mode as the fix
```

`CreateSymbolicLinkW` needs admin rights or Developer Mode. Junctions need no privilege
and are genuine reparse points, so a directory link behaves correctly and `test -L`
reports true honestly.

Rejected: falling back to hard links for files. A hard link is not a symlink — `test -L`
is false and deleting the target does not dangle it — so a script that creates a link and
then inspects it gets silently wrong answers. Failing loudly is better, per D20 and D26.

**`test -L` checks the reparse *tag*, not the attribute.** True only for
`IO_REPARSE_TAG_SYMLINK` and `IO_REPARSE_TAG_MOUNT_POINT`. Checking
`FILE_ATTRIBUTE_REPARSE_POINT` instead would make every App Execution Alias look like a
symlink (D46), and `.lnk` shortcuts are a shell concept, not a filesystem one.

cash follows symlinks and junctions transparently, as Win32 does by default.

### D28 — REVOKED. Reserved names stay devices

**Originally:** `nul`, `con`, `com1`–`com9`, `lpt1`–`lpt9`, `aux`, `prn` would be treated
as ordinary files, so `touch nul` created a file called `nul` exactly as on Linux. The
reasoning was sound — a POSIX script never means the device, it writes `/dev/null` — and
D29's `\?\` prefix made it mechanically possible.

**Revoked on implementation evidence.** It was built, and it worked at the shell layer:

```
echo x > nul      created a real file
[ -f nul ]        true
ls                nul
```

and then broke in a way that made it worse than not having it:

```
rm -f nul         builtin rm: fails, file remains
cat nul           builtin cat: "Incorrect function"
```

**D28 and D48 are irreconcilable.** The bundled uutils builtins (D48) are third-party
code with their own argument parsing; they open paths directly rather than through cash's
file-open boundary, so they never see the `\?\` form that makes a reserved name
reachable. Making them see it would mean cash translating path-shaped arguments for 165
commands whose argument grammars it does not know — which is precisely the guessing that
D4 exists to forbid, and the road that ends at `MSYS2_ARG_CONV_EXCL`.

So the choice was between them, and D48 wins on measured value: self-contained, 2.2x
faster, and it dissolves most of D35's problem. D28 was a niche convenience that, half
implemented, produced files cash's own `rm` could not delete.

**Standing behaviour:** `nul` and friends are devices, as every other Windows program
sees them. `/dev/null` remains the way to discard output (D7), which is what a POSIX
script writes anyway — so nothing the target use case needs is lost.

`cash_win32::path::is_reserved_name` is kept: `cash doctor` (D35) can warn when a script
redirects to a bare `nul` expecting a file.

### D29 — Always `\\?\`-prefixed UTF-16 paths internally

No `MAX_PATH` limit, on any machine, regardless of the long-path registry setting. This
matters in practice: nested `node_modules` and Terraform plugin cache directories blow
past 260 characters routinely, and the resulting error surfaces far from its cause.

Three consequences:

- `\\?\` requires **absolute, backslash-separated** paths, so the conversion from D3's
  canonical `C:/foo` happens here. This is the one place backslashes are mandatory.
- `\\?\` **disables OS path normalization** — `.` and `..` are no longer resolved for
  you. cash must canonicalize them itself, which D10's chokepoints already require.
- It bypasses reserved-name parsing, which is what makes D28 work — and why D7 needs an
  explicit carve-out for `/dev/null`.

**How it is met (2026-10-04).** Rust's standard library adds the `\\?\` prefix itself to
a path too long for the plain form, so cash's file operations take paths of any length
without a conversion of their own; `cash_win32::path::to_extended`, written for this,
had no caller and is gone. One limit no prefix lifts: Windows starts no process in a
folder whose path is longer than 258 characters. cash's builtins worked in such a folder,
but every program failed, its bundled tools included, with `C:\…\cash.exe: Not a
directory`. A program is now started in the folder's 8.3 short name
(`…\CASH-L~2\AAAAAA~1`), which Windows keeps on most volumes and which fits; it sees that
spelling as its working directory. Where a folder has no short name that fits, the
command fails with status 126 and says why (`cash_win32::path::process_directory`; the
user, 2026-10-04). Git Bash's own tools work there; its native programs fail, with an
error that says the folder is too long.

### D30 — `.bashrc` errors warn per occurrence, then summarise

Each failing command prints as bash would, and startup continues. Startup then ends with
a one-line summary — `3 errors in ~/.bashrc` — so the fact cannot be missed.

Directly serves D24's accepted risk: a Linux-authored `.bashrc` will fail here, and
silence is what would make that expensive to diagnose.

### D31 — Environment variable lookup is case-insensitive; POSIX names canonicalise to uppercase

Windows environment blocks conventionally use `Path`, `ProgramFiles`, `Temp`,
`UserProfile`. Bash lookup is case-sensitive. Left alone, `$PATH` would come back
**empty** on a Windows-supplied environment while `$Path` worked — probably the
highest-frequency breakage available.

- Lookup is case-insensitive: `$PATH`, `$Path` and `$path` all resolve. An exact match
  always wins; of several names that differ only in case, the first in sorted order
  does, which is the upper-case spelling when there is one. It reaches every variable,
  the shell's own included: an unset `$seconds` reads `SECONDS` (kept, 2026-10-03).
- Well-known POSIX names (`PATH`, `HOME`, `TMPDIR`, `USER`, …) normalise to uppercase on
  import, so scripts see the spelling they expect.

Accepted cost: a script using `$Foo` and `$FOO` as distinct variables breaks. This does
not happen in practice, and Windows itself could not represent it.

### D32 — `.bat` / `.cmd` arguments are escaped for cmd, with documented residual gaps

D8 routes `.bat` and `.cmd` through `cmd.exe /d /s /c`, and cmd re-parses the command
line with its own rules: `&`, `|`, `^`, `<`, `>` are special, and `%VAR%` expansion can
pull values into an argument that the script never intended.

cash applies cmd's caret-escaping and neutralises `%` expansion so ordinary arguments
survive intact. cmd's parser has genuinely ambiguous corners, so "correct for all inputs"
is not achievable — the documentation must state where it stops rather than implying
total fidelity.

The quotes around each argument remain structural; they are not caret-escaped. This is
required for PATH-resolved scripts under directories such as `Microsoft VS Code`, where
turning the quotes into literals makes `cmd.exe` split the script path at the first space.

Not a conflict with D4: D4 forbids rewriting arguments *semantically* (guessing at
paths). This is quoting for a specific, known interpreter that cash is deliberately
invoking.

### D33 — Maximal file sharing, and POSIX delete semantics

- cash opens files with `FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE`, so an
  external `rm` can always delete a file cash holds open.
- cash's own deletes use `FILE_DISPOSITION_FLAG_POSIX_SEMANTICS`, which unlinks the name
  immediately while existing handles stay valid — real POSIX unlink on Windows.

Makes `tool > log & rm log` behave as it does on Linux, rather than failing with a
sharing violation.

No fallback path is needed: D47 sets the floor above the version where this appeared.

### D34 — `chmod -x` warns and returns 0

Revoking execute properly means either editing an inherited Full Control ACE — which
requires breaking inheritance — or adding a Deny ACE, which is blunt and can lock you out
of your own file. Neither is worth it.

`chmod -x` therefore warns and returns 0. It does not silently lie (rejected, per D20 and
D26) and it does not construct Deny ACEs.

Asymmetric with D23's `chmod +x`, which genuinely can add the ACE if absent. That
asymmetry is inherent to Windows ACLs, not a design choice.

### D35 — Userland-agnostic, with a `cash doctor` diagnostic

cash requires no particular userland and resolves whatever is on `PATH` (D8). It ships a
`cash doctor` that reports what it found and names the fix.

**Why a diagnostic rather than a dependency.** "The userland is solved" is not quite
true. MS Coreutils ships coreutils + `find`/`xargs` + `grep` — and **no `sed`, no `awk`**,
since those are separate GNU projects. So no single install is a complete answer, and the
failure modes are quiet. A real machine surveyed during design had:

- every Unix command shimmed to **BusyBox** — `sed`, `awk`, `xargs`, `ls`, `cat`,
  `grep`, `sort`, `find` — so `awk` was a POSIX subset with no gawk extensions
- GnuWin32 `coreutils` and GNU `grep` installed but **unreachable**, their shims
  overwritten by a later BusyBox install
- `find` and `sort` shadowed again by DOS `C:\WINDOWS\system32\find.exe` and `sort.exe`
  winning on `PATH` (position 6 versus 42), so `find . -name '*.tf'` silently hit the DOS
  tool

None of that announces itself. `cash doctor` should detect:

- missing `sed` / `awk`
- BusyBox applets shadowing fuller implementations
- DOS `find` / `sort` / `more` winning over the Unix ones
- shims pointing somewhere unexpected
- App Execution Aliases whose target app is not installed, which silently open the
  Microsoft Store instead of running — the notorious `python` behaviour (D46)

Recommended (documented, never enforced): MS Coreutils via winget, plus GNU `sed` and
`gawk` from any source.

**BusyBox, since cash carries most of the userland.** `sed`, `awk`, `find`, `xargs` and
the rest are cash's own now, so a BusyBox shim for them is never reached. For the tools
cash does not carry, doctor flags a BusyBox applet only where it breaks real scripts, a
curated list (BusyBox gap analysis, Q10) with each reason checked against BusyBox 1.38:
`grep`/`egrep`/`fgrep` (no `-P`, no `--include`), `diff` (no `-y`, no `--color`), `make`
(`$(shell …)` expands to nothing), `tar` (no `--transform`), `xz` (decompresses only),
`nc` (no `-z`), `wget` (`-T` crashes it). When a full copy of the tool is further down
`PATH` — often Git for Windows' — doctor says to move its directory ahead of the shims;
otherwise it names what to install. Applets whose reduction nothing notices (`cal`,
`cat`) are not reported. Doctor also notes the System32 tools cash shadows on purpose
(`ping`, D57; `reset`, D55) and how to reach them.

### D36 — Prompt commands get their own job, as every command does (measured)

*Superseded on 2026-10-03, by measurement.* D36 had `PROMPT_COMMAND` and `PS1` command
substitutions share a long-lived, pooled job object, for fear that D6's per-job setup on
the prompt's path (`CreateJobObject`, `SetInformationJobObject`,
`AssignProcessToJobObject`, the handle close, and the registry sweep) would slow every
prompt. The pool was never built, and it would save nothing anyone could see:
`cargo run --release -p cash-win32 --example prompt_job_cost` measures the setup at
0.18 ms a spawn, against 128 ms for a Starship prompt start to finish, about 0.14% of it,
far under the 5 ms that was to justify building it (TODO, decided 2026-10-02).

So prompt commands are contained as every command is, each in its own job, and they are
no longer an exception to D6's guarantee. What makes Starship slower here than on Linux
is process creation itself, not the job.

### D37 — Starship is a stated requirement, tested from M1

Not a compatibility bonus. The user runs Starship on every machine and every shell, so
"Starship renders a correct prompt" is a first-class acceptance test from M1, not a
manual smoke check.

brush already advertises Starship compatibility, along with `PROMPT_COMMAND`, right
prompts, DEBUG/ERR/EXIT traps, fzf/atuin, and zsh-style precmd/preexec hooks. Starship's
bash init needs `PROMPT_COMMAND` and the DEBUG trap for command-duration timing, so this
is inherited via D9 rather than built.

When D18's replacement trigger fires, Starship becomes a **regression risk**. The M1 test
exists precisely to have a baseline at that moment.

**Status: working, and tested.** It needed one fix that was not obvious from the spec.
`starship init bash` emits

```
eval -- "$('/c/Program Files/starship/bin/starship.exe' init bash --print-full-init)"
```

— a *Unix-spelled command path*. cash rejected it with `command not found` on a path that
plainly existed, because D3's acceptance had been wired into file operations but not into
command resolution.

That was an oversight rather than a tension with D4: D4 forbids rewriting *arguments*,
where cash cannot know which are paths. A command name is unambiguous, and resolving it
is exactly what D3 means by "paths cash resolves itself". Commands now accept every
spelling, and the prompt renders in full — directory, git branch, git status, language
versions.

Starship is Rust and emits `\n`, so D20 never touches its output.

### D38 — Ships via winget and Scoop, with a Windows Terminal profile fragment

- **winget** — matches how MS Coreutils installs, and where a Windows-native shell is
  looked for.
- **Scoop** — matches the existing workflow of the target user.
- **Windows Terminal profile fragment** — cash appears automatically as a terminal
  profile on install. No Unix-ported shell bothers with this, and it is most of the
  difference between "a binary you run" and "your shell".

Cost accepted: three artifacts to keep in sync per release. `cargo install` remains
available and is the M0–M2 distribution channel, when the audience is one person.

Refined with the user, 2026-09-28 (ROADMAP item 18, [packaging
evaluation](research/packaging-evaluation.md)): **Scoop first**, from the author's own
bucket `tomcoolpxl/scoop-bucket`; **winget later**, with a per-user **Inno Setup**
installer, since a portable winget package cannot upgrade a running `cash.exe` and the
installer type cannot change once published. Both install as a normal user; nothing
needs admin. The Terminal fragment is written on every install and removed on uninstall.
`cash` is taken on crates.io, so `cargo install` needs another crate name.

The fragment is cash's to write: `cash --terminal-profile` puts `cash.json` and the logo
in `%LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\cash\`, one profile named `cash`
running the `cash.exe` the command ran as, spelled as it was started (under Scoop
`apps\cash\current\cash.exe`, which follows upgrades); `cash --remove-terminal-profile`
deletes the folder. The profile's `icon` is the logo's address on GitHub, and the logo is
written beside the fragment under that address's file name, `cash_logo_small.png`:
Terminal 1.24 and later loads a fragment's images from its own folder by that name,
earlier versions take only a web address. `cash.exe` carries the same logo as its icon.
The profile starts in `%USERPROFILE%`: without it Terminal starts a fragment's profile in
its own folder, `C:\WINDOWS\system32`. Its GUID, `{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}`,
is the one Terminal derives from the names, written out. A user whose `newTabMenu` lists
profiles one by one, with no `remainingProfiles` entry, would never see the profile in
the + menu (the first Scoop install did exactly that), and a fragment cannot change the
menu; so `--terminal-profile` adds one entry to the end of that list in each Terminal
install's `settings.json`, and `--remove-terminal-profile` cuts it out again. The file is
JSON with comments and the user's own, so it is never rewritten whole: only that entry's
text changes, and Terminal reloads the file, fragments included, at once.
Terminal ships no Nerd Font, so the profile asks for one the machine has, decided with
the user, 2026-09-28: first a Cascadia one (the registry's font list), to look like
Terminal's own, `CaskaydiaMono Nerd Font Mono`, Microsoft's `Cascadia Mono NF`,
`CaskaydiaCove Nerd Font Mono` or `Cascadia Code NF`; else the Nerd Font the user
already has Terminal draw in, the default profile's first; else any Nerd Font GDI
reports as fixed pitch, `Nerd Font Mono` ones first. A Nerd Font of a proportional face
(Ubuntu, Noto Sans) is never picked from what is merely installed. Nerd Fonts 3 list
short names (`UbuntuSansMono NFM`) where Terminal wants the long one (`UbuntuSansMono
Nerd Font Mono`), so the name is turned. With none, the profile names no font, `ls
--icons` shows no icons there (D67), and Scoop suggests `nerd-fonts/CascadiaMono-NF`
rather than installing it; `cash --terminal-profile` run again after installing a font
picks it up.
Scoop's manifest (`packaging/scoop/cash.json`) runs them from its `post_install` and, on
a real uninstall only, its `pre_uninstall`; it also offers carapace and the Nerd Font
through `suggest` (D63). `cash.exe` links the C runtime statically, so no
installer has to bring the Visual C++ Redistributable, which needs admin.

### D39 — Terminal shell integration is on by default

cash emits OSC 133 (prompt / command / exit-code marks) and OSC 9;9 (cwd reporting).
Windows Terminal and VS Code then give clickable command blocks, jump-to-previous-command,
exit-code decorations, and cwd-aware new tabs. brush already does VS Code integration, so
this is partly inherited.

Serves §1's "pleasant enough to replace your shell" bar for very little implementation.

- OSC 9;9 reports the Windows spelling, `C:\...`: Windows Terminal reuses it for
  "duplicate tab here", and Microsoft's own example for bash under WSL sends
  `wslpath -w "$PWD"`. Windows Terminal is detected by `WT_SESSION` and gets OSC 133
  marks with it; VS Code (`TERM_PROGRAM=vscode`) gets its own OSC 633, which carries the
  command line too; iTerm2 and WezTerm get OSC 133 and OSC 7 (a `file://` URL). Until
  2026-10-04 Windows Terminal got only 633 (XC-18).

### D40 — Completion is case-insensitive and auto-quotes

- Case-insensitive, consistent with D16.
- Auto-quotes results containing spaces or shell metacharacters.
  `C:/Program Files` is the most common path on Windows and breaks unquoted every time.
- Completes both spellings: `C:/Prog<TAB>` and `/c/Prog<TAB>` both work, per D3.

**Quoting continues in the style the word was started in** (decided 2026-09-26, with
ROADMAP item 15's ConPTY harness):

| Typed | Tab gives |
| --- | --- |
| `cd my` | `cd 'my dir/'`, the cursor before the closing quote |
| `cd 'my` | `cd 'my dir/'` |
| `cd "my` | `cd "my dir/"` |
| `cd my\ ` | `cd my\ dir/`, as Bash completes |
| `ls it` | `ls "it's here.txt"` |

- With no style started, single quotes, as PowerShell completes: inside them `\`, `$`,
  `` ` `` and `!` are literal, which Windows names need (`$Recycle.Bin`,
  `'C:\Program Files\'`). A name holding a `'` gets double quotes, with `$`, `` ` ``,
  `\` and `"` escaped inside.
- A quote already typed is kept, even where the candidate would not otherwise need
  one: someone who typed `"` meant it. The span a completion replaces includes that
  quote, so the candidate carries it (`ls "Prog<TAB>` → `"Program Files/"`, never
  `""Program Files"`).
- A backslash counts as a started style only before a character that needs escaping,
  so `C:\Prog` is a path, not an escape.
- A quoted directory keeps its `/` inside the quotes and gets no trailing space. The
  cursor is left before the closing quote, so Enter runs the line as it is and typing or
  Tab carries on inside it: `cd 'my dir/in<TAB>` gives `cd 'my dir/inner/'`, the old
  closing quote replaced. Reedline has no cursor offset for a suggestion; cash wraps its
  columnar menu to move the cursor, and gives the completer the whole line so it sees
  the quote after the cursor.
- A file gets its closing quote and a space, finished like any other word.
- Candidates that are not file names (a completion function's) are left as they are,
  unless it asks for quoting with `compopt -o fullquote` (Bash 5.3), which quotes them
  by these same rules. `-o noquote` turns quoting off and wins over `fullquote`.
- Inside single quotes a `\` is a path separator, not an escape, so `'my\ dir` means a
  name with a backslash in it, as it does in Bash.

The ConPTY harness (`crates/cash/tests/pty_oracle.rs`) records each of these beside Git
Bash 5.3's screen. It also found that the interactive layer had been quoting D40's
already-quoted candidates a second time, `\"alpha\ beta.txt\"`: D40 had been tested only
by calling the completer directly.

**Several candidates: the shared part first.** A Tab inserts what all candidates share
(`ga<TAB>` → `gam`) and does nothing more. The next Tab shows them in the grid, where
further Tabs move through them. With nothing shared left to insert, the first Tab shows
them. Bash beeps on the second Tab and lists on the third. Until 2026-10-02 one Tab
both inserted and showed the grid, against this paragraph; the user chose the second Tab
(vendor/reedline/CASH-PATCHES.md, patch 7).

**The grid opens on the history hint's candidate** (the user, 2026-10-02). With
`cd docker-labs/` in history, `cd dock` shows the grey hint `er-labs/`. The first Tab
inserts the shared `er`. The second opens the grid on `docker-labs/`, not on its first
candidate, so Tab, Tab, Enter takes what the hint showed:
- The chosen candidate is the one that turns the line into the longest start of the
  hinted line. A file, which gets a space after it, counts only where the hinted line
  has whitespace after the name or ends there.
- Without a hint, or when no candidate leads there, the grid opens on the first
  candidate.
- Tab and the arrows move on from the chosen one.

fish, zsh-autosuggestions and PowerShell open their menus at the top: this is cash's
own (vendor/reedline/CASH-PATCHES.md, patch 6).

Quoting applies only to filename candidates, and `complete -o noquote` turns it off, as
in bash. Windows-only, so D43's differential suite is untouched.

**Generated completion scripts work, which needs two shims.** `docker completion bash`,
`kubectl completion bash`, `gh completion -s bash` and every other Cobra-generated
script — most of the Go ecosystem — call `_get_comp_words_by_ref` and `_filedir`. Those
live in the `bash-completion` *package*, not in bash. Linux distributions install it;
Windows has nothing to install, and Git for Windows does not ship it either, so on that
platform sourcing `docker completion bash` succeeds and then silently completes
nothing at all.

cash therefore defines both helpers itself in interactive shells, before rc files so a
user who does install bash-completion overrides them. Scripts and `-c` commands do not
get them, so their function table matches Bash's. Measured after: `docker ru` → `run`,
`kubectl get po` → `pods`, `gh pr cr` → `create`, and `docker run --rm <TAB>` lists the
local image tags. clap-generated scripts (rustup, cargo, ripgrep) never needed the
shims and keep working.

Stated limitation: the shim splits the command line on whitespace only. A quoted
argument containing a space is seen as two words — completion still works, the request
forwarded to the tool is merely split where the quote would have held it together.

### D41 — BOM stripped on read; console code page set to UTF-8

- **BOM:** `EF BB BF` at the start of a script is stripped before parsing. Windows editors
  write it, and it otherwise breaks the shebang and prefixes the first token with three
  invisible bytes. Same family as D7's CRLF tolerance.
- **Console:** cash sets the console to UTF-8 (65001).

**Scope of the code page decision, stated honestly.** It fixes console *display* and
console-attached children. It does **not** affect pipeline bytes: when cash pipes a
child's output it reads raw bytes and the console code page is never consulted. Runtimes
that pick an encoding for piped output generally consult the **ANSI** code page instead —
Python writing to a pipe uses `cp1252` regardless of `SetConsoleOutputCP`.

**Risk assessment.** Anything Go or Rust writes Unicode via `WriteConsoleW` or emits UTF-8
directly, so `terraform`, `gh`, `argocd`, `istioctl`, `kustomize`, `k3d`, `ripgrep`, MS
Coreutils, `jq`, `curl` and `git` are unaffected. Residual exposure:

- **JDK before 18** used the legacy code page for `System.out` (UTF-8 became default in
  JDK 18). Relevant where `openjdk17` / `maven` are in use.
- `.bat` with non-ASCII, especially `for /f` — already best-effort per D32.
- DOS-lineage `findstr` / `find` / `sort` / `more` handle Unicode poorly regardless.
- Old .NET **Framework** console apps (not .NET 5+).

The pre-1803 console-input bug under 65001 is long fixed and below D47's floor. Windows
ships a system-wide UTF-8 option and PowerShell 7 defaults to UTF-8, so this follows the
platform.

**Rejected:** injecting `PYTHONUTF8`, `JAVA_TOOL_OPTIONS` and similar into the child
environment to fix the pipe case. It would work, and it directly contradicts D5 — PATH is
the only variable cash touches. That known-list magic is what D5 exists to prevent.

### D42 — Elevated processes are a documented hole in D6

**D6's guarantee does not cross an integrity boundary.** This is a platform limit, not a
design choice:

- An elevated child cannot be started with `CreateProcessW`. Elevation goes through
  `ShellExecuteEx` with the `runas` verb, routed via the AppInfo service — so the
  elevated process is not even cash's child.
- `AssignProcessToJobObject` fails against it regardless, because a medium-integrity
  process cannot acquire `PROCESS_SET_QUOTA` / `PROCESS_TERMINATE` on a high-integrity
  one.

So an elevated program outlives cash, and cash says so rather than pretending otherwise.
`elevate` (D45) is the first-class verb, so the elevation happens where cash can see it
rather than behind its back through an external runner: it warns, on standard error,
that the program runs outside cash's job object and will not be reaped when cash exits,
and then asks UAC to start it (`-q` drops the warning). It does not record the program
or try to end it at exit. An earlier text of this decision said cash would, by pid; by
the second point above the attempt could not succeed, and `elevate` never made it
(found in 1.3.12, while moving `elevate` to `ShellExecuteExW`).

UAC elevation is in real daily use and refusing it outright is worse, so it stays, as a
hole stated plainly. The documentation must not overstate D6 because of this.

### D43 — Language conformance on Linux CI; Windows has its own acceptance corpus

**Amended 2026-09-28: cash is Windows-only.** The Linux half of this decision is gone.
bash already runs natively on Linux, so a Linux cash would add nothing, and brush's Unix
code was removed together with the Linux CI job. brush's differential suite (2,588
cases) went with it: it needed a Unix PTY, so it never ran on Windows, and the Linux job
had only been running the 13 probes in `tests/bash52-differential.sh`. Language
conformance now rests on the inline-expectation cases in `crates/cash/tests/cases`,
the Windows acceptance corpus, and the targeted Bash 5.2/5.3 probes recorded under
`research/bash-reference/`. The original decision follows.

brush's ~2,500 differential tests run the same script under brush and real bash. They
stay on Linux CI, where a real bash exists, and cover the *language*.

Windows behaviour is tested against its own corpus of real scripts (§6), not against a
bash reference.

**Why not Git Bash or WSL as a Windows reference.** Both would produce diffs dominated by
false positives, because the §4 divergences are *deliberate* and a bash reference would
report every one as a failure — and Git Bash adds MSYS2 path translation while WSL adds
Linux filesystem semantics over `/mnt/c`. The comparison would measure the wrong thing.

### D44 — History appends immediately, concurrency-safe

Each command is appended as it runs, not written at exit.

Two Windows-shaped reasons, both consequences of other decisions:

- **D6 terminates without unwinding.** Bash's write-on-exit means a force-kill, a Task
  Manager kill, or D6 teardown silently loses the whole session's history.
- Multiple Windows Terminal tabs are concurrent writers to one file, so appends must be
  safe under contention rather than last-writer-wins.

Cost accepted: one file append per command, on the command path.

The default file is `~/.cash_history`. Deleting it cannot clear a running shell's
in-memory list, and a later command may recreate it; `history -c; history -w` clears both.
All interactive test harnesses disable persistence or use a temporary home so test input
cannot contaminate a developer's history.

- OPEN: Atuin-compatible backend as an option. brush already supports Atuin, so this is
  likely cheap, but it is a second history path to keep consistent.

### D45 — cash-specific builtins

Four, all justified by decisions made above rather than invented:

| Builtin | Purpose |
|---|---|
| `winpath` | Explicit conversion between `C:/foo`, `C:\foo` and `/c/foo`. D4 forbids cash rewriting arguments automatically, so this is the deliberate escape hatch when a tool genuinely needs backslashes. |
| `detach` | Deliberate job-object breakaway — start something meant to outlive the shell. |
| `elevate` | UAC elevation as a first-class verb, so cash can warn that the child escapes D6 and will outlive it (D42). It does not wait for the command; its status says only that it started, and it says so. |
| `sudo` | A command elevated in this terminal (2026-10-04). Since 2026-10-10 cash elevates the command itself (the user's pick, `research/sudo-in-terminal-design.md`): an unelevated `cash --invoke-bundled --sudo-elevate` asks UAC, each time, for an elevated cash, which attaches to its console and takes its other standard handles with `DuplicateHandle`, so the command has this terminal's keys, output and Ctrl-C, redirections and pipes work, and its status comes back; it ends if the shell goes first. Declined, `sudo` says so with 1. Windows' own `sudo` and UAC's new window are no longer used. Since 2026-10-11 (the user's pick) `-u USER` and `su USER` too are cash's own: `cash --invoke-bundled --sudo-as` asks USER's password at the console without echo, then starts `cash --invoke-bundled --sudo-owner - COMMAND` as USER with `CreateProcessWithLogonW` (USER's profile and environment). USER's cash can neither open the caller nor attach to this console (another account may not; both `Access denied`), so a redirected handle is handed over by inheritance (`STARTF_USESTDHANDLES`) and a console handle is relayed — USER's cash reads and writes a pipe, the caller relaying the bytes to and from the real console on its own threads, as gsudo does (a byte relay: full-screen console apps lose VT fidelity). It is put in a kill-on-close job, so it dies with a killed shell; USER is granted this window station and desktop while the command runs (`winstation.rs`), put back after, so a windowed program can show; and when the shell's folder is under the caller's profile, which USER cannot enter (`ERROR_DIRECTORY`), the command runs from the system temp (`%SystemRoot%\Temp`), else the drive root, with a note. gsudo is no longer used. cash chooses what runs, as the shell would: `sudo bash` is cash (`sudo.exe` found WSL's `bash.exe`), `sudo ls` cash's `ls` through an elevated cash, a batch file or a script through cash; a function is not run, as a Unix `sudo` runs none. `sudo NAME=value COMMAND` passes the variables on. `-i` is a login shell in the account's home (or COMMAND run by one), `-s` a shell in this folder; `-u USER` that account at its usual level, its password asked (`-u root` is elevation), refused before any password when USER cannot read the program (cash under `~/scoop` is its owner's and the administrators' only: `scoop install -g cash` or Program Files makes it readable; cash never copies itself somewhere shared); `-E` passes the exported variables; `-n` fails with `sudo: a password is required` unless already elevated; `-v`, `-k` and `-K` are kept for scripts, with no cache to open or close; `-l` says who you are, whether you are an administrator, what elevates, the cache, and whether other accounts can run cash.exe; `-e FILE...` is `sudoedit`; `-h`/`--help` shows the usage, as `help sudo` does; other options are refused. The command starts in the shell's folder, on a mapped drive by its UNC path. Elevated, the command runs under `cash --invoke-bundled --sudo-owner SID`, which makes the user who asked the default owner of what it creates, not Administrators (the user, 2026-10-04; Windows allows it when that user approved with their own account); when another account approved, it prints `sudo: running as DOMAIN\ADMIN, not you; files and ~ are theirs` once. Already elevated, it runs the command here. Tab completes the command after `sudo` and its options as a command and its arguments as its own, and the user after `-u` from the local accounts; `cash doctor` says which tool elevates, gsudo's cache, a note for a standard account, and whether other accounts can read cash.exe. `elevate`, `sudo`, `su` and `detach` find what runs in one place. |
| `su` | Unix's `su`: `su`, `su -` and `su root` are this account elevated (Windows has no root account), `su USER` that account; `-`/`-l`/`--login` a login shell in the account's home, `-c COMMAND` a command, `-s SHELL` that shell (found as the shell would run it) instead of cash, `-m`/`-p`/`--preserve-environment` the exported variables as `sudo -E` passes them, `-h`/`--help` the usage. It goes through what `sudo` uses, with its checks and its ownership of created files. Tab completes the user from the local accounts. The shell is a new cash: Windows raises no process already running. |
| `sudoedit` | Unix's `sudoedit`, also `sudo -e` (2026-10-04): each FILE copied to `$TEMP` under its own name, `$SUDO_EDITOR`, `$VISUAL` or `$EDITOR` (the first set; Notepad without one) run unelevated on the copies and waited for; each copy that changed is written into its file by an elevated cash (`cat -- COPY > FILE`), so the file keeps its access list and owner; a file that did not exist is created; an unchanged file is left alone. `-u USER` and `-n` as for `sudo`. |
| `start` | Open a file or URL with its default handler — the Windows `xdg-open`. |
| `abbr` | fish's abbreviations, which the prompt expands in place (D60). Added later, and not Windows-specific. |
| `prevd`, `nextd`, `cdh` | fish's folder history, also on Alt-← and Alt-→ (D62). Added later, and not Windows-specific. |
| `croot` | A file and folder picker, inline below the command line on Alt-E, and as a command that prints the pick (D73, 2026-10-05). |
| `xdg-open` | `start` under the name cross-platform scripts try first, with xdg-open's exit codes (D74, 2026-10-06). |
| `pbcopy`, `pbpaste` | The clipboard as Unicode text, with LF turned into CRLF on the way in and back on the way out; `clip.exe` writes the console code page and Windows has no paste command (D74, 2026-10-06). |

**`detach` has a cost, listed as an exception in D6.** For a child to leave the session
job, that job must be created with `JOB_OBJECT_LIMIT_BREAKAWAY_OK`, which means *any*
child can request breakaway by passing `CREATE_BREAKAWAY_FROM_JOB`. Having `detach`
therefore slightly weakens D6's guarantee for everyone. Accepted as the price of an
explicit escape hatch.

### D46 — App Execution Aliases work, with three specific caveats

Store aliases in `%LOCALAPPDATA%\Microsoft\WindowsApps` — `python.exe`, `bash.exe` and
dozens of others — are **0-byte files carrying `IO_REPARSE_TAG_APPEXECLINK`**. They
execute because `CreateProcessW` resolves that reparse tag natively, so cash launching one
is no different from launching any other `.exe`. No special support required.

1. **Resolve by extension before reading the file.** A read of an AppExecLink returns
   nothing. Handled by D8's ordering.
2. **`test -L` must check the reparse tag, not the attribute.** Handled by D27.
3. **`[ -s python.exe ]` is false.** They are genuinely 0 bytes, so a script that
   sanity-checks a binary by size gets a wrong answer. Unfixable; documented in §4.

- OPEN: whether a packaged app launched via an alias lands in cash's job object (D6).
  Full-trust desktop-bridge packages like Store Python are ordinary children and should
  inherit it; true UWP apps launched via a broker may behave like D42's elevation case.
  Needs testing, not reasoning.

### D47 — Minimum platform: Windows 10 1809; Windows 11 is the target

Several decisions depend on a platform floor, and leaving it unstated left fallback paths
open that do not need to exist:

| Requirement | Introduced | Ref |
|---|---|---|
| Nested job objects | Windows 8 | D6 |
| Unprivileged symlinks with Developer Mode | 1703 | D27 |
| `FILE_DISPOSITION_FLAG_POSIX_SEMANTICS` | 1709 | D33 |
| Console input fixed under CP 65001 | 1803 | D41 |
| ConPTY | **1809** | §1 |

ConPTY binds. Windows 10 reached end of support in October 2025, so **Windows 11 is the
supported target** and 1809 is the theoretical floor. No pre-1709 fallback is needed for
D33, and D41's historical console bug is below the floor.

### D48 — coreutils are bundled as in-process builtins, on by default

cash ships 165 uutils coreutils as builtins rather than requiring them on `PATH`.
Measured before deciding:

| | |
|---|---|
| Binary size | 11 MB → **15 MB** |
| Utilities gained | **165** |
| `PATH=""` then `echo x \| cat` | **works** |
| 300 × `echo x \| cat > /dev/null` | **4s** builtin vs **9s** spawning |

The speed difference is the Windows story in miniature: process creation costs
milliseconds here, so a pipeline in a loop pays for every spawn. That cost is why
BusyBox ends up installed on Windows machines in the first place.

**It largely dissolves D35's problem.** Everything `cash doctor` found on the machine
this was designed against — BusyBox applets masquerading as `sed`, DOS `find` winning
from System32, Store aliases opening the Microsoft Store — stops applying to the
commands cash carries itself.

**Builtins take precedence, as in bash**, with bash's own escape hatch: `enable -n cat`
disables the builtin and the `PATH` executable is used instead. Verified working.

**A bundled tool names itself (2026-10-02).** It runs as a process of its own,
`cash.exe --invoke-bundled cut -f1`, and uutils names a tool after the first word of
the process's command line, which Windows makes the program's path and lets no one
choose: `cut` said `cash.exe: you must specify a list of bytes, characters, or fields`
and `Try 'C:/…/cash.exe --help'`. In that process, and with the import patching D7
describes for the `/dev` names, `GetCommandLineW` answers with the command line the tool
would have had on its own, `"cut" "-f1"`, so it says `cut:` and `Try 'cut --help'` as
GNU's does (`cash_win32::cmdline`).

cash's `xargs` follows this precedence too, including its default `echo` command.
Each builtin invocation uses an isolated shell copy, so state changes and `exit`
do not affect the caller. Arguments remain data, without another shell parse.
An explicit executable path bypasses builtin lookup; `enable -n` restores external
lookup for a bare name. This extends GNU xargs's external-command model to cash's
bundled userland.

Normally "builtin shadows the real tool" is a silent-substitution hazard of exactly the
kind D20 and D26 reject. Here it mostly is not: **Microsoft's Coreutils *is* uutils**,
and so are these builtins. On the recommended setup (D35) the builtin and the `PATH`
copy are the same implementation, so preferring the builtin substitutes nothing and
merely skips the spawn. The divergence is real only for someone running GNU coreutils,
and `enable -n` covers them.

**`ps` is cash's own, because uutils has none.** `ps` belongs to procps, not
coreutils, so the bundle does not carry it — and what a Windows machine supplies instead
is the MSYS `ps` from Git for Windows, which is worse than nothing here: it lists only
MSYS processes, omitting every native program, and the numbers it prints are MSYS pids
that `kill` cannot use. It looks like it worked. That is the exact failure D35 exists to
name, and the answer D48 prefers is to carry the tool.

The native process builtins implement the useful common surface: `ps`, `ps -e`,
`ps -ef`, `ps -efj`, `ps aux`, `pgrep -P`/`--parent`, `pstree`, and `top`. A bare `ps`
lists the shell's own descendants, because Linux's `ps` shows the processes attached to
your terminal and Windows has no controlling terminal to filter by. `$PID`, `$BASHPID`,
`$$`, and `$PPID` use the same native Windows IDs these tools print and `kill` accepts.
The `-efj` view substitutes Windows base priority for Linux job-control fields that have
no machine-wide Windows equivalent. Deliberately absent: `ps -o` format strings and the
full command line of another process, which means reading that process's PEB.

`top`, as decided with the user on 2026-09-29:

- **The screen.** It draws on the terminal's alternate screen with the cursor hidden,
  and writes each frame in one piece over the last: from the top-left corner, each line
  overwrites the one before and erases the rest of itself, inside synchronized output
  (mode 2026, which Windows Terminal has had since 1.23). It used to clear the screen
  before every frame, which flickered, and Windows Terminal scrolls a cleared screen into
  the scrollback, so each refresh left a copy there. Quitting puts the screen back.
- **The interval** is 3 seconds, procps-ng's, and `d` changes it; the first frame comes
  after a 0.3-second sample rather than a whole interval.
- **The load average** takes Linux's definition, tasks running or waiting for a
  processor: the processors busy over the interval, exactly, from the machine's CPU
  times, plus the threads ready to run at the sample, counted from one
  `NtQuerySystemInformation(SystemProcessInformation)` snapshot of every process and
  thread (the Idle process's threads stand for idle processors and are left out). The
  sample is smoothed over 1, 5 and 15 minutes from the moment `top` starts. It replaced
  the `\System\Processor Queue Length` performance counter, which cost about 350 ms of CPU
  at every start (measured on 361 processes) and is missing on some Windows builds. The
  snapshot also replaces the Toolhelp process list, so a refresh costs no more. Microsoft
  documents the function and the members read, with the warning that they may change in
  a future Windows; it is looked up at run time as Microsoft advises, and without it the
  load average is not shown. The same function gives per-processor times for `1`.
- **Cost**, measured with 360 processes, release builds: the first frame went from 406
  to 62 ms of CPU, and each further refresh from 27 to 5 ms, chiefly because a process's
  account is now looked up once rather than at every refresh.
- **Keys** beyond sorting and scrolling, from procps's `top` and NTop: `V` a tree, off by
  default; `1` a CPU meter or line per processor; `o` or `/` only names containing some
  text, `=` all again; `d` the interval; `k` a signal by `kill`'s rules (never to the
  shell itself). `j` and `k` no longer scroll: procps's `k` is kill.
- **Meters and colour**, also decided with the user on 2026-09-29. CPU and memory show
  as meters by default, as htop and NTop draw them: user time green, kernel time red,
  memory in use green, commit yellow, the reading at the right end. They form one
  aligned block, as wide as the column header up to `COMMAND` (or the screen, if
  narrower), so a reading stays by its bar however wide the window is; labels are padded
  so every `[` lines up, and Mem and Commit have a line each. `t` and `m` cycle each
  between meters, procps's text and hidden, as procps-ng's `t` and `m` do; with `1` the
  processors' meters come two to a row within that width. Colour is on when `top` draws on a terminal
  and off in batch mode or with `NO_COLOR`; `z` toggles it: the summary's figures bold,
  the owner coloured as `ls` colours a file's owner, the sort column cyan, and a process
  that ran in the last sample bold. Batch mode keeps procps's plain text.
- **Columns stay put.** `TIME+` coarsens as time grows, as procps does (`1423:07.21`
  becomes `1423:07`, then hours, days and weeks), and `%CPU` drops its decimal past
  999.9, so no value pushes `COMMAND` out of line.

The native `tree` covers the common, cheap filesystem view: `tree [DIRECTORY]`, `-a`,
`-d`, `-L LEVEL`, `-f`, `--dirsfirst`, and `--noreport`. It prints directory links and
junctions as leaves instead of following them, preventing cycles and walks outside the
requested root.

**The gap stays open: still no `sed`, no `awk`.** Microsoft Coreutils packages
uutils coreutils, findutils, and grep, but not the separate `uutils/sed` project or an
AWK. Cash is therefore not yet a complete userland in one executable, and D35's
diagnostic keeps its job. Confirmed: `type sed` reports not found even with the bundle
enabled. The selected candidates and required compatibility work are documented in
[`research/uutils-sed-evaluation.md`](research/uutils-sed-evaluation.md) and
[`research/posixutils-rs-evaluation.md`](research/posixutils-rs-evaluation.md).

**Bundled output is rendered through D3.** The utilities are portable Rust, so the few
that *construct* an absolute path — `mktemp`, `realpath`, `readlink` — spell it the way
Windows does. `mktemp -d` printing `C:\Users\me\AppData\Local\Temp\tmp.AbCdEf` is how
that spelling reaches a script at all, since nearly every use is `d=$(mktemp -d)`; and
the first command downstream that reads `\` as an escape rather than a separator
destroys it. `find "$d" | xargs grep` yields `C:Usersme...` and reports no matches —
wrong answer, no error, exactly what D20 and D26 exist to prevent.

So the dispatcher redirects its own standard output for those three, and renders each
line through D3 before writing it on. The allowlist stays minimal and is justified by a
single test: does the utility *build* the path, or merely echo back one it was given?
`find`, `grep -l`, `wc`, `du`, `dirname` and `basename` all echo, so they are correct as
soon as their inputs are, and rewriting their output would silently alter data the user
chose. Standard error is never captured — diagnostics keep their ordering, and they are
not paths. `--help` and `--version` skip the capture: that output is prose, and an
argument parser that exits the process while printing it would otherwise strand the text
in the capture file.

A temp file rather than a pipe, because nothing drains a pipe while the utility runs and
64 KiB of `realpath` output would deadlock.

- Resolved: `sed` and `awk` are bundled (imports of uutils/sed and posixutils-rs AWK,
  maintained in-tree). They target POSIX rather than GNU, and their line-ending policy is
  D49.

### D49 — Bundled `sed` and `awk` keep CRLF files CRLF; `CASH_EOL=lf` is Linux

Windows text files end their lines with CRLF, and a text tool that treats the CR as data
makes `s/foo$/bar/` or `$NF == "x"` fail on them while printing identically, the D20
problem again. So by default the bundled `sed` and `awk`, reading a line that ends in
CRLF, match and split it without the CR and write it back with the CR: an edited Windows
file stays a CRLF file, and an LF file stays LF.

Two exceptions give the CR back to the script, because there the script asked for it:

1. **The program names a carriage return.** A regular expression (or `y` source in sed)
   containing `\r`, `\x0D`, octal `\015` in awk, or a literal CR makes CR ordinary data
   for that whole run. The classic dos2unix one-liners (`sed 's/\r$//'`,
   `awk '{sub(/\r$/, "")} 1'`) therefore do what they say. `s/.$//` names no CR and still
   removes the last visible character.
2. **Linux mode.** `CASH_EOL=lf` in the environment (case-insensitive; any other value is
   the default) makes CR ordinary data for every run, exactly as on Linux, so `$` no longer
   matches before it. For a single sed run, `-b`/`--binary` does the same; it is the
   option GNU sed's Windows builds use for this. `printf` in awk is always written verbatim.

**Everything sed writes for a line ends as that line does (2026-10-04).** The script
sees only `\n`. A CRLF line's pattern space, the newlines joining CRLF lines in it
(`N`, `G`, `H`, and a newline the script made, `s/,/\n/`: which newline stood for which
line end is not kept through an edit), the text of `a`, `i` and `c`, `=`'s number,
`F`'s name, `l`'s listing lines, the output of `e` and `s///e`, and the lines of `w`
files all end in CRLF. A CRLF the program wrote itself is kept, never doubled. A last
line without an end takes the input's ending as far as it is known. `r` and `R` insert
the other file's bytes untouched, with no end supplied after them, as GNU sed does. The
exceptions above and `-z` leave everything as GNU sed writes it. `printf 'a\r\nb\r\n' |
sed N` gave `a\nb\r\n` until then.

Converting files between the two conventions is the job of the bundled `dos2unix` and
`unix2dos`, not a side effect of editing.

### D50 — `fuser` and `lsof`: documented sources, and the handle walk for what a process holds

Windows has no per-process descriptor table a user can read. The system-wide handle list,
`NtQuerySystemInformation(SystemExtendedHandleInformation)`, is undocumented, and naming a
handle can hang. cash first left it out; since 2026-10-04 (the user) `lsof` walks it as
Sysinternals' handle.exe and System Informer do, because it is the only way to answer
"what does this process have open" and has been stable since Windows XP:

| Question | Source |
|---|---|
| Who holds this file | the Restart Manager (`RmGetList`), the API behind Explorer's "file in use"; for files it cannot answer (system DLLs, where it fails with an invalid handle), each process's image and module list |
| Who owns this port | IP Helper's owner-PID TCP and UDP tables, IPv4 and IPv6 |
| What does this process have | its executable (`txt`), loaded modules (`mem`), sockets, and the files and folders it holds open, from the handle walk (`cash_win32::handles`) |

The walk copies each File handle into cash (`DuplicateHandle`) and names it
(`GetFinalPathNameByHandleW`) on a worker thread; a handle that does not answer within
500 ms (a file object locked by a blocked synchronous read) is skipped and counted, and
another worker carries on. Unelevated it sees the processes the user may open, as Linux
`lsof` without root, and a warning counts the others; elevated, the walking thread alone
turns `SeDebugPrivilege` on, and only protected processes stay closed.

Output follows psmisc `fuser` 23.7 and lsof 4.99.7 (process ids alone on standard output
for `fuser` and `lsof -t`; lsof's nine columns). What Windows cannot say is shown as
unknown rather than invented: lsof's FD is `txt`, `mem`, `0` to `2` for the standard
handles (2026-10-05: their values, read from each process's parameters, mark them, and on
a pipe or device they are `FIFO pipe`, `CHR /dev/tty` or `CHR /dev/null`, told apart by
device type, which never waits on a read), another handle's value, or `-`, each with
`r`, `w` or `u`; DEVICE and NODE are `-` (NODE is `TCP`/`UDP` for sockets). `lsof`
with no selection lists every process. A folder (`lsof +D`, `+d` or a folder named, and
`fuser DIR`) is answered from the walk, the executables and modules of every process,
and for the processes the walk cannot open, the Restart Manager over the files below,
asked in halves; past 20,000 entries that last part is skipped with a warning. Refused,
with a message: `fuser -m`/`-c`/`-M` (mount points), `-w` (write access is not
reported), `lsof -U` (Unix sockets cannot be listed), and lsof's
field output and repeat modes.

### D51 — `ss` is iproute2's layout over the Windows socket tables

Linux scripts use `ss` for "is anything listening" (`ss -ltn | grep -q ':5432 '`) and for
`ss -tulpn`. Windows' `netstat.exe` answers the same questions with other flags and
another layout, so those scripts fail. cash's `ss` reads the same owner-PID socket tables
as `fuser` and `lsof` (D50) and prints them in iproute2 7.2's layout: its columns and
widths, `Netid` only when several socket tables are selected, `State` only when more than
one state is, listeners first in each family, service names for ports unless `-n`, host
names with `-r`, an IPv6 scope as an interface name without spaces (`%ethernet_32769`),
and status 0 when nothing matches. Options are parsed as iproute2's getopt_long parses
them (unambiguous prefixes such as `--num`, its error messages, status 255), and the
selection follows iproute2's `main`: without `-a` or `-l` the default view leaves out
listeners, TIME-WAIT and SYN-RECV, `exclude` alone starts from every state, `-A` takes
`!table`, and `-s` with a selection prints the whole summary and then the list. State
filters (iproute2's names, `bound-inactive` included) and the `sport`/`dport`/`src`/`dst`
expression language (with `and`, `or`, `not`, parentheses, prefixes and `-F FILE`) are
supported; a socket without a peer has peer `0.0.0.0:0`, and what iproute2's grammar
rejects gets its `bison bellows` message.

Where Windows differs (decided in research/ss-evaluation.md): `-p` prints `fd=-`, and for
a service hosted in `svchost.exe` adds `service=NAME` from the socket's owning module; UDP
sockets have no peer and are always `UNCONN`. Recv-Q and Send-Q are `0`, so column
positions stay the same, except for a connection whose extended statistics Windows
collects (an elevated `ss -i` switched them on): there Recv-Q is the data received and
not yet read (`CurAppRQueue`) and Send-Q, as in Linux, the data the peer has not yet
acknowledged, sent or not (`CurRetxQueue` plus `CurAppWQueue`). `-i` prints, on the
second line iproute2 uses (or the same line with `-O`), only what Windows has, under the
name of the Linux value it is, in iproute2's order: unelevated, the MSS values of the SYN
(`mss` is the one the peer offered, `advmss` this end's), with a note that the rest needs
an elevated shell; elevated, it first switches `GetPerTcpConnectionEStats` collection on
for each listed connection that lacks it (it stays on until the connection closes, and
counts start then, which a note says), and adds `wscale`, `rto`, `rtt`, the current
`mss`, `cwnd` and `ssthresh` in segments, the byte and segment counts, `send`,
`dsack_dups`, `reordering`, `notsent`, `minrtt`, `snd_wnd` and `rcv_wnd`. The congestion
algorithm, `pmtu`, `rcvmss`, `lastsnd` and the other timers have nothing behind them and
are left out; listeners and UDP sockets get no `-i` line. `dev NAME` (a name such as
`ethernet_32769`, an alias such as `Ethernet`, or an index) matches an IPv6 socket's
scope, as Linux matches the device a socket is bound to; Windows has no per-socket device,
so every IPv4 and unscoped IPv6 socket has none, as an unbound Linux socket has (`dev 0`
matches them, `dev != NAME` keeps them). `-K` closes connections with `SetTcpEntry`,
which Windows allows only elevated and only for IPv4: unelevated it is refused before
anything is done, and an IPv6 match is reported as not closable. `-B` (bound-inactive
sockets, netstat's `BOUND`) comes from the undocumented
`InternalGetBoundTcpEndpointTable`, looked up at run time; Windows lists there the
binding of every socket that went on to connect or listen, so those are left out. A
dual-mode IPv6 socket shows as two lines (`0.0.0.0` and `[::]`) where Linux prints one
`*`: the tables do not say whether a socket is dual-mode. Options with nothing behind
them (`-x`, `-e`, `-m`, `-o`, `--tos`, `--cgroup`, other socket families) are refused by
name, and a netstat habit such as `ss -ano` gets a hint with the `ss` spelling.
`netstat.exe` is not shadowed.

### D52 — MSYS2 and Cygwin programs get their arguments in Cygwin's encoding

A Windows program splits its one command line into words itself. Programs linked against
`msys-2.0.dll` or `cygwin1.dll`, which is every tool in Git for Windows' `usr/bin`, split
it by Cygwin's rules: `'` quotes as well as `"`, a backslash escapes outside quotes, a
word with a wildcard is globbed against the working directory, and `@file` reads a
response file. The Microsoft C runtime's encoding, which every program used to get, does
not survive that: `JSON.sh`'s `"[^[:cntrl:]"\\]*"|[[:space:]]+` reached Git's `grep` as
`\[^[:cntrl:]"\]*"|[[:space:]]+`, and `new<LF>line` arrived as two words.

cash reads a native executable's import table (cached by path and modification time) and,
for an MSYS2 or Cygwin program, encodes each argument the way Cygwin's `build_argv` and
`globify` decode it. A word with nothing Cygwin acts on passes unchanged. Anything else is
double-quoted with `"` and `\` escaped. A drive-letter path is the exception: Cygwin keeps
its backslashes literal, so only the characters that need it are quoted, one at a time.
A program named without a path, as `exec grep` and Git's own `egrep` script name it, is
looked up along the shell's PATH first, so the check sees what `CreateProcess` will
run; `xargs` and `find -exec` do the same along the process's PATH. The bundled `env`
and `timeout` (uutils) spawn their command with `std::process::Command` themselves, which
only writes the Microsoft encoding, so when their command is an MSYS2 program the
dispatcher names cash as the command instead: `cash --invoke-bundled --msys-relay TOOL
PROGRAM ARGS…`. cash decodes the Microsoft way, so the relay receives the arguments
intact, looks the program up again in the environment and directory the tool set up, and
spawns it with them encoded for MSYS2. The tool's options are left to the tool; only
`env -S` strings are split first, as `env` itself does, so a command inside one is found.
The program runs in a kill-on-close job under the relay, so `timeout --foreground`, which
kills only its own child, still kills it, and the relay ignores console events so the
program's own response to Ctrl-C decides the exit status the relay passes back. The other
command-running tools need nothing: `nohup` is a cash builtin that runs its command
through `cash -c`, and `nice`, `stdbuf` and `chroot` are not bundled.
`crates/cash/tests/msys_args.rs` round-trips hostile arguments through Git's `printf.exe`.

### D53 — `winpaths`: at the prompt, an unquoted `C:\…` means the path

D3 keeps bash's lexing, so an unquoted `cd C:\Users\me` reaches `cd` as `C:Usersme`, and
every path pasted from Explorer, a PowerShell prompt or a Windows error message fails
unless it is quoted. Quoting on paste was the first idea, and it cannot work on Windows:
crossterm has no bracketed paste there, so a paste arrives as ordinary keystrokes with
nothing marking where it starts or ends.

`shopt winpaths` changes the word parser instead. In a word that starts with a drive and
a backslash, and only in its leading unquoted run, a backslash is kept wherever bash's
escape would only have removed it:

- before a letter, a digit or one of `. _ - $ @ + % , = ^ ~ # [ ] { }`, both the
  backslash and the character are literal (`C:\$Recycle.Bin`, `C:\ProgramData\{GUID}`);
- before `*` or `?`, which no Windows file name contains, the backslash is kept and the
  wildcard still globs (`C:\logs\*.txt`);
- before anything else (a space, a quote, `!`, `(`, an operator) it is bash's escape:
  `C:\Program\ Files` is one word.

The run ends at the first quote, expansion or substitution, and only a word's first
characters can trigger the rule, so `x=C:\y` and `--dir=C:\y` keep bash's meaning.
Completion and highlighting (D59) follow the same rule. A word like `C:\Users` lexes
under bash to something no script means, so the rule takes nothing a script relies on,
but it is still a divergence and the same line can mean different things typed and in a
script. It is therefore **on by default in interactive shells and off otherwise**;
`shopt -u winpaths` in `.cashrc` turns it off at the prompt too. A failed `cd` whose
target has lost its backslashes (`cd: C:Usersme: No such file or directory`) prints a
hint naming the option.

A word that starts with `\\` and a server's name (a letter, digit, `.`, `_`, `-` or
`$`), a UNC path pasted as `\\server\share\dir` or `\\wsl$\Ubuntu\home`, follows the
same rule, its leading `\\` kept as two backslashes (the user, 2026-10-10, by pick list:
bash reads it as `\servershare`, which no script means either). Tab completes a path
typed with backslashes in that spelling.

### D54 — `pkill`, `pidof` and `killall`: one set of name rules, `kill`'s signals

Native builtins, with procps-ng 4.0.7's (`pkill`, `pidof`) and psmisc 23.7's (`killall`)
output and exit statuses, checked against the real tools. Stock Windows has none of them
(`taskkill` is another interface), and on a machine with BusyBox on `PATH` all three are
BusyBox shims reporting pids `kill` cannot use.

The rules were decided for the family together with `pgrep`, which now shares them
(research/busybox-gap-analysis.md, Q2):

- Names match **case-insensitively**, as Windows image names do, and **`.exe` is
  optional** on both sides: `pidof notepad`, `pidof NOTEPAD.EXE` and `pgrep -x notepad`
  all find `notepad.exe`. A pattern's own `.exe` is set aside too, so `pkill exe` does
  not mean every process.
- Signals go through `kill`'s path: `TERM`, the default, asks and escalates (D21);
  `KILL` terminates; `STOP`/`CONT` suspend and resume (D19); each pid alone (D22). A
  process is held open while it is signalled, and one that started after the listing is
  passed over, so the signal reaches the process that was listed and no later owner of
  its pid; `killall -w` waits for those processes, not for their pids (D22).
- The kill family never signals the shell running it, pid 0 and 4, images Windows cannot
  survive losing (`csrss`, `wininit`, `winlogon`, `smss`, `services`, `lsass`, …), or a
  process running as `SYSTEM`, `LOCAL SERVICE` or `NETWORK SERVICE`. A process the user
  may not open is skipped rather than reported as a failure. `killall -v` lists both
  kinds of skip, and a name whose every match was skipped says so instead of "no process
  found".
- `pidof` prints the highest pid first, procps' order; on Windows, which reuses pids,
  that is not newest first. `pkill -n`/`-o` use real start times.
- Refused with a reason: `-f`/`--full` (a command line lives in another process's
  memory, refused as `ps -o args` is), the user, group, session and terminal selectors,
  and `killall -i`.

### D55 — `getopt`, `rev`, `clear` and `reset`

Native builtins from the BusyBox gap analysis, each checked against the real tool:

- **`getopt`** is util-linux's (2.42.3): GNU `getopt_long` parsing with clusters,
  `--name=value`, unique prefixes, `-a`, the `+`/`-` option-string prefixes and
  `POSIXLY_CORRECT`; every error reported and parsing continued; `-T` exits 4. It
  shadows Git's MSYS `getopt.exe`, whose output is the same but which receives its
  arguments through D52's encoding. The output is single-quoted, which cash's parser
  reads as bash does. `-s csh` and `-s tcsh` are refused.
- **`rev`** is util-linux's: code points reversed, invalid UTF-8 kept byte by byte,
  `-0`/`--zero`, and `-` as a file name. A CRLF line keeps its `\r` at the end (D20);
  util-linux moves it to the front.
- **`clear`** and **`reset`** write the VT sequences ncurses 6.6 writes for
  `xterm-256color` instead of consulting terminfo, since ConPTY and Windows Terminal
  interpret VT; `-T` or a terminal type that is not xterm- or vt-like is refused. `reset`
  also puts back cooked input, VT output and the UTF-8 code page (D41), which a program
  that died in raw mode leaves wrong and no escape sequence can fix. It shadows
  `C:\Windows\System32\reset.exe`, the Remote Desktop `reset session` command, which
  `command -p` or its full path still reaches.

`crates/cash/tests/oracle` holds the case scripts and util-linux's output; the tests run
the same scripts under cash and name each deliberate difference in place.

### D56 — `bc` is POSIX bc, from posixutils-rs, and says so about GNU's

`bc` is the standard way shell scripts do decimal arithmetic, and neither Windows nor Git
for Windows ships one. Cash bundles posixutils-rs' POSIX bc (`crates/cash-bc`, MIT,
imported at a recorded revision as `awk` was), running as `cash --invoke-bundled bc`.

POSIX only, decided in the BusyBox gap analysis (Q4), and no `dc`. GNU bc's language
extensions are not implemented; when a program fails to parse, `bc` names the first one
it used (`print`, `read()`, `else`, `&&`, `||`, `!`, `#` comments, `last`, `halt`,
`continue`, `limits`, or a multi-letter name), so a script written for GNU bc fails with
a reason. GNU's options that change nothing for POSIX bc (`-q`, `-s`, `-w`) are accepted,
`--mathlib` is `-l`, and `-i` forces interactive error recovery.

What changed from upstream (listed in `crates/cash-bc/README.md`):

- a CRLF line is read as an LF line (D20);
- a value below one prints without a leading zero, `.33` and `-.5`, as GNU, BSD and
  BusyBox bc print it and scripts compare it; upstream printed `0.33`, and its tests
  were updated with the change;
- a function body may start on the line of its `{`, so `define f(x) { return (x); }`
  is one line. POSIX requires a newline there, but GNU (outside `-s`), BSD and
  BusyBox bc do not, and short functions are written this way. It is the one GNU
  extension accepted.

Upstream's suite runs unchanged apart from its helpers (`crates/cash/tests/bc.rs`), and
cash's changes have their own tests (`bc_cash.rs`).

**Against GNU bc** (ROADMAP item 13): `crates/cash/tests/oracle/bc_cases.sh` ran under
GNU bc 1.07.1 to make `bc_cases.out`, and `bc_matches_gnu_bc` runs it under cash. Its 52
cases cover the 70-column wrap of long numbers, `obase` from 2 to 1000, `ibase`,
scale, modulus and powers, the math library at scale 25 to 50, functions, arrays and
control flow. Output matches GNU's byte for byte. One difference is deliberate: after
a runtime or syntax error, GNU bc carries on and exits 0, and cash carries on and exits
1, so `set -e` and `|| die` catch a failed calculation; POSIX leaves the status
unspecified.

### D57 — `ping` takes Linux's flags, and shadows `ping.exe`

`ping -c 1 host && …` is how scripts test a host, and Windows' `ping.exe` reads `-c` as a
routing compartment: unelevated it refuses with "Access denied", so every host looks
down. Cash carries its own `ping` with iputils' flags as they are (Q9) — `-c` count,
`-i` interval, `-W` reply timeout, `-w` deadline, `-s` size, `-t` TTL, `-n` numeric,
`-q`, `-4`/`-6`, `-D`, `-O`, `-a`, `-H` — and iputils 20250605's output, messages and
exit statuses (0 with a reply, 1 with none, 2 on error).

It runs as a bundled command, a process in the foreground job, so Ctrl-C (or Ctrl-Break)
reaches it and it prints its statistics before exiting. Echoes go through
`IcmpSendEcho2`/`Icmp6SendEcho2`, as Windows gives unprivileged programs no raw socket;
the options that need one (`-f`, `-l`, `-p`, `-R`, `-T`, `-I`, socket options) are
refused by name. Where Windows differs:

- one echo is in flight at a time, so an earlier echo waits at most the interval and a
  reply slower than that counts as lost; the last echo waits the full `-W` (or iputils'
  ten-second linger);
- an IPv6 reply carries no hop limit through this API, so its line has no `ttl=`;
- `-s` stops at 65500 data bytes, Windows' limit, not iputils' 65507.

`ping.exe` stays reachable by its path, as `ping.exe` (a name the builtin does not
match), or after `enable -n ping`. A Windows habit is caught where it would otherwise
misfire: `ping -n 3 host` is iputils' "numeric output, through hop 3" and would ping
forever, so a second operand is refused, with a hint that `ping.exe`'s `-n 3` is `-c 3`
here. `cash doctor` reports the shadow. Cash's own tests used `ping -n 20 127.0.0.1` as a
long-running Windows process; they now name `ping.exe`, as scripts relying on Windows'
flags must.

### D58 — `which` gives a runnable path for the commands cash carries

`LS=$(which ls); "$LS" -la` is how scripts capture a tool, and under cash `ls` is a
builtin with no file behind it: `which ls` printed `ls: shell builtin`, and `"$LS"` ran a
command by that name. Shipping launcher executables was declined (files cash would have
to keep in step with itself), and so was printing a command line (`cash --invoke-bundled
ls`), which a quoted `"$LS"` cannot run.

`which` prints a **virtual path** instead: cash's own executable with the name appended,
`C:/…/cash.exe/ls`. No file can be there — `cash.exe` is a file, not a directory — so the
path names nothing else, and it says which cash runs the command. It is one word, so it
survives quoting. Cash recognises it wherever a command is run: typed or from a variable
it runs the builtin by name (never a function, as a path never names one); where a process
is needed — `exec`, `xargs`, `find -exec` — cash re-enters itself as
`cash -c '"$0" "$@"' ls …`; and `[ -x "$LS" ]` is true. A disabled builtin's path
(`enable -n ls`) still runs cash's own command, by re-entry, as a path to a program would.

It applies to the commands cash carries that are programs elsewhere (`ls`, `sed`, `ps`,
`rev`). Bash's own builtins (Bash 5.3's `enable -a`: `cd`, `read`, `export`, …) are no
program anywhere, and `which cd` still says `cd: shell builtin`. `type -P ls` is
unchanged: it searches `PATH` only, as bash's does. The limit is plain: a program outside
cash — Python's `subprocess`, a `.bat` file — cannot run the path; nothing without a file
on disk could offer that.

### D59 — Syntax highlighting is on, and never waits on the disk

fish highlights the line as it is typed and marks a command that does not exist before
Enter is pressed; that is most of why a mistyped command costs nothing there. The
highlighter came with brush and was complete, but off: its default was tied to brush's
`experimental` build feature, which cash does not enable. It is now on by default.
`--enable-highlighting=false`, or `syntax-highlighting = false` under `[ui]` in
`%APPDATA%\cash\config.toml`, turns it off.

It could not simply be switched on. To colour the command word it asked whether the name
is on `PATH`, on every keystroke once the cursor had left the word, and on Windows that
question is expensive: each `PATH` directory is tried with each `PATHEXT` extension. On the
machine this was measured on, with 86 directories and 13 extensions, a name found nowhere
took **127 ms** a keystroke, `code` 60 ms and `starship` 28 ms.

So the highlighter asks a background listing instead (`cash_core::pathindex`). Each `PATH`
directory is listed once, on a thread of its own, and a lookup is a set membership test.
A directory is listed again when its modification time changes, which NTFS bumps when an
entry is added, removed or renamed; those times are checked in the background at most
every two seconds, never on the typing thread. Until a listing is ready the command word
keeps the neutral colour rather than waiting. A name matched without an extension
(`egrep`) has its contents checked once, as execution checks them (D46).

The listing can lag a program installed a moment ago. That is acceptable for a colour and
would not be for running a command, so nothing that executes consults it; command
resolution still probes the disk as before.

A command spelled as a path is looked at directly, one cheap question of a local disk,
except on the network: a UNC path or a mapped network drive keeps the neutral colour, as
asking it can wait on a server, for seconds when it is offline, on every keystroke
(PI-10, 2026-10-04).

Every command of a line is checked, not only the first: a control operator (`;`, `&`,
`&&`, `||`, `|`, `|&`, `(`, newline) or a reserved word that precedes a command (`if`,
`then`, `do`, `!`, …) starts another, so `ls | nosuch` marks `nosuch`.

A command spelled with a path is checked as the path the shell will run: its quotes
removed, its backslashes read as D53 reads them, resolved as every path is (D3, so
`/c/tools/x.exe` works), and executable by the rule `type` uses (a directory is not). A
pasted `C:\Windows\System32\curl.exe` is therefore one path in one colour, not a run of
escapes; with `winpaths` off its backslashes are escapes again and it is marked missing,
because it would run as `C:WindowsSystem32curl.exe`. A path that waits on an expansion
(`~/bin/x`, `$HOME/bin/x`) keeps the neutral colour.

### D60 — `abbr`: fish's abbreviations

An alias is replaced when the command runs, so history keeps `gco` and the screen never
shows what ran. fish's abbreviations are replaced on the line itself: `abbr -a gco git
checkout`, then `gco` followed by Space or Enter becomes `git checkout`, visibly, and that
is what runs and what history keeps. Aliases stay as they are; nothing turns them into
abbreviations.

`abbr` takes fish's options so a line from `config.fish` works: `-a`/`--add` (the default
when names are given), `-e`, `-s`/`--show` (the default without arguments; it prints
`abbr -a -- gco 'git checkout'` lines that define them again), `-l`, `-q`, `-r`, and
`--position command|anywhere`. `-g` and `-U`, fish's old scope flags, are accepted and
ignored, as current fish ignores them. fish's `--regex`, `--function` and `--set-cursor`
are refused by name. Like fish, the expansion is the remaining arguments joined by spaces,
after the shell has removed their quoting: an expansion that needs quotes of its own is
quoted whole.

An abbreviation expands only as the command word — first on the line, or after a control
operator or a reserved word that precedes a command (D59's rule) — unless it was defined
with `--position anywhere`, and never inside quotes or a comment. Reedline, the line
editor, already expands abbreviations on Space and Enter and asks the highlighter whether a
position allows it; cash hands it the table before each prompt and answers from the shell's
own table, which also refuses a name `abbr -e` removed since. Reedline tried expansion only
when a read began with the space, so keys read together (`o` and Space, when typing
outpaces the repaint) never expanded; cash's copy tries after every space (reedline patch
4). A bracketed paste is not typed spaces, so pasted text never expands, as in fish.

`abbr` is a cash-only builtin (D45). In real Bash a `.bashrc` line calling it fails, so a
file shared between shells guards it: `command -v abbr >/dev/null && abbr -a gco git
checkout`.

### D61 — A collapsing prompt: `CASH_TRANSIENT_PS1`

fish 4.1 and Starship's fish, PowerShell and cmd integrations can redraw a finished
command's prompt as something short, so scrollback shows the commands rather than a
two-line status block above each one. Cash does it when `CASH_TRANSIENT_PS1` is set, and
not otherwise. The variable takes everything PS1 takes — backslash escapes, `$(…)`,
`$?` — and is expanded with PS1, before the line is read, so both describe the same moment.
When Enter is pressed, the prompt above the line is redrawn as it, without the right-side
prompt, as fish does. It carries the same terminal-integration marks as the prompt it
replaces (D39), so Windows Terminal's command marks still line up.

A variable, not a function hook (decided): it reads like PS1, and one line in `.bashrc`
does it. Two useful values:

```bash
CASH_TRANSIENT_PS1='\[\e[32m\]❯\[\e[0m\] '                                  # free
CASH_TRANSIENT_PS1='$(starship module character --status="$STARSHIP_CMD_STATUS")' # Starship's own
```

The cost is one more prompt expansion per prompt, and nothing unless the variable is set.
A `$(…)` in it runs each time: measured here, `starship module character` is about 30 ms
(Starship's full prompt in a git repository, about 200 ms, is the larger cost).
`STARSHIP_CMD_STATUS` is where Starship's bash integration keeps the last status, so the
character turns red after a failure as the full prompt's does.

### D62 — Folder history: `prevd`, `nextd`, `cdh`, and Alt-← / Alt-→

fish keeps where the shell has been and steps through it like a browser's back and forward.
Cash keeps the same: every change of working folder (`cd`, `pushd`, `popd`, `cdh`) records
the folder it left, keeping the last 25 as fish does, and drops the folders ahead of it.
`prevd [-l] [N]` and `nextd [-l] [N]` walk the record without adding to it; `-l` lists it
as fish's `dirh` does. `cdh` lists the recent folders, most recent as 1, and reads a number;
`cdh FOLDER` is `cd FOLDER`. All three are cash-only builtins (D45).

**Keys.** On an empty line, Alt-← runs `prevd` and Alt-→ runs `nextd`, and the prompt is
drawn afresh in the new folder. On a line with text they move a word, as fish's
`prevd-or-backward-word` does. Reedline binds keys without seeing the line, so the keys
carry a marker the input backend resolves once it can: on an empty line it runs the
command the way a `bind -x` key does; otherwise it moves the cursor and goes on reading,
without recomposing the prompt. At either end of the history the key does nothing, quietly.
Windows Terminal uses Alt-arrows to move between split panes, so they reach cash only in an
unsplit tab. Readline has no default binding for Alt-arrows, so nothing Bash does is lost.

Building this found that keys typed straight after a key bound to a command were dropped
when read in the same batch as it (`bind -x` keys included); Reedline patch 5 keeps them.

### D63 — Completions from carapace, when it is installed

fish ships completions for hundreds of commands, each candidate with a description. Cash has
Bash's machinery (`complete`, `compgen`, the bash-completion shims of D40), and scripts a
user sources work; but nothing loads them, and a bash script has no descriptions to give.
[carapace-bin](https://github.com/carapace-sh/carapace-bin) (MIT) completes 729 commands on
Windows — git, gh, winget, scoop, docker, kubectl, terraform, cargo, npm, dotnet, pwsh — with
a description for each candidate.

**Not bundled** (decided). carapace is Go, so it cannot be linked in, and it is 90 MB
unpacked (17 MB zipped), three times `cash.exe`. Cash uses it when it is there: beside
`cash.exe` first, then on `PATH`. `cash doctor` says whether it was found and how to install
it (`scoop install extras/carapace-bin`). When the one found is a Scoop shim, cash starts the
`carapace.exe` its `.shim` file names instead, saving a second process on every Tab (about
70 ms measured); anything else — winget's link, a copy placed by hand, a shim whose target
is gone — is started as found (D25).

**When it is asked.** On Tab, for an argument of a command that has no completion of its
own — no `complete` spec for it and no `complete -D` default — and that carapace lists
(`carapace --list`, read once and remembered while `PATH` is unchanged). Cash runs
`carapace <command> export <command> <arguments…> <word>` in the shell's folder, with the
environment a command would get, and shows its values with their descriptions; values are
quoted as cash quotes its own (D40), and carapace's `nospace` decides the trailing space.
When carapace answers nothing, fails, or takes over three seconds, cash's own candidates
stand. The command's name, a word inside quotes, and a word after a redirection are left to
cash.

A carapace that is installed but does not answer its list is not written off. Measured
right after `scoop install`, its first starts took 4.7 s, 3.1 s and 1.4 s while Windows
scanned the new program, then about 0.5 s; a first Tab then gave up, and cash had remembered
carapace as unusable until the next session. It is now asked again after 30 seconds, so a
broken install costs one Tab in 30 rather than every Tab. The same happens after an update.

**Cost.** Nothing per keystroke; only a Tab. Measured on the machine this was written on,
carapace starts in about 50 ms, a Tab costs 85–100 ms where carapace has the list itself
(`winget`, `terraform`, `gh pr`) and 220–450 ms where it runs the tool (`git checkout`,
`docker`), the same order as the tools' own bash completion scripts, which run the tool too.
The first Tab of a session also reads carapace's list, once.

### D64 — `CHLD` is cash's own event: a trap runs once per child reaped

Windows delivers no `SIGCHLD`, and cash refused `trap … CHLD` for that reason. But the
event behind it is one cash sees: every child process it starts, it waits for. Bash 5.3's
item 1.ii (job notices after a trap) needed it, and scripts that keep a pool of background
jobs count them with a `CHLD` trap. Decided 2026-09-27: emulate it.

- **When it runs.** At the points Bash runs a pending trap: after a foreground command,
  inside `wait`, when a background job starts, and at the prompt, where the trap runs
  before the job notices, as in Bash 5.3.
- **How often.** Once for each process of a foreground command or pipeline that cash
  started and reaped, and once for each background job, as `cmd &` is in Bash. Checked
  against Git Bash 5.3.15 with external programs, the counts are Bash's.
- **Where it does not.** Only the shell itself runs it: a subshell or background job runs
  in a copy of the shell, whose trap Bash resets. Children that the handler itself starts
  are not counted, or a handler that runs a command would set itself off without end.
- **`kill -CHLD`** to another process does what POSIX's default does: nothing.

What cannot match is what has no process in cash: a bundled tool (`ls`, `cat`) and a
command substitution run inside cash, so they reap no child and raise no `CHLD`; §4
divergence 38.

### D65 — `cash --link-tools`: hard links that programs outside cash can run

D58's `C:/…/cash.exe/ls` runs only inside cash: Python's `subprocess`, a `.bat` file or
an IDE can start only a real file. `cash --link-tools [DIR]` makes hard links to
`cash.exe`, one per tool it carries that is not a Bash builtin (`ls.exe`, `sort.exe`, 126
of them), and cash started under a linked name runs as that tool, as BusyBox does. A
hard link is the same file under another name: no space, no admin on a folder the user
can write, and the same hash and signature for AppLocker and App Control. Decided with
the user, 2026-09-27:

- **Folder.** DIR, or `bin` next to `cash.exe`, made when missing and used as it is when
  it exists. A DIR on another drive cannot hold a hard link, and is an error that says so.
- **Knowing its own.** A manifest, `.cash-links`, lists the links cash made. Started as
  `NAME.exe`, cash runs as `NAME` only when the manifest in its folder lists it; a copy
  under a tool's name that nobody linked is still the shell. Re-running refreshes cash's
  links to the running `cash.exe`, which is how an upgrade reaches them; any other file of
  that name is left alone and listed.
- **Windows' names.** Every tool is linked; the nine that System32 also has (`expand find
  hostname ping reset sort timeout where whoami`) are named, since with the folder before
  System32 on PATH a `.bat` calling `find` gets cash's.
- **PATH** changes only when asked (revised 2026-09-28, ROADMAP item 18). `--add-to-path`
  puts the folder first on the user `Path` in `HKCU\Environment`, unless an entry
  already names it. Windows puts the user `Path` after the machine's, so System32's nine
  still win for `.bat` files, while cash's tools win over the user's other Unix tools
  (on the author's machine, 82 BusyBox shims in Scoop). The value keeps its type,
  `REG_EXPAND_SZ`, and its `%VAR%` entries unexpanded, and the change is announced with
  `WM_SETTINGCHANGE`. This replaced the PowerShell line the command printed, which
  rewrote the user `Path` as a `REG_SZ` with every `%USERPROFILE%` expanded.
  `--unlink-tools [DIR]` removes cash's links, the entry, and the folder when nothing
  else is in it.
- **A link Windows will not delete.** Windows deletes a name of a running program while
  the file has other names, but not its last one: links to an old `cash.exe` that is
  itself gone, while a program still runs one of them. Such a link is renamed aside
  (`ls.exe.cash-old-1`, which no command lookup finds), and a later run deletes it. NTFS
  gives a file at most 1023 names, 126 of them per links folder; that error says so.
- **`which`** prints a tool's link when one is on PATH, a real file, ahead of the virtual
  path. **`cash doctor`** checks each links folder on PATH and warns when a link is no
  longer the running `cash.exe` (an upgrade replaced it), naming the command that
  refreshes them.

A linked tool runs as `cash -c '"$0" "$@"' NAME ARGS`, the re-entry D58's paths use. Two
things follow from the exe being the link. cash re-enters its own exe for a bundled tool
and for `sh`; the child would take itself for the tool again, so the cash that starts it
says the name it is to run under in `CASH_ARGV0`, which the child reads and removes, and a
link asked for by its own name is not told (BIN-09; `CASH_LINKED_TOOL_EXE` did this until
2026-10-04, and every descendant inherited it). And Windows
looks for a program started by a bare name in the folder of the exe that starts it: cash
starts the Windows programs it uses (`whoami`, `quser`) by their System32 path, or in a
links folder `whoami.exe` would be cash, starting `whoami.exe` again as it came up.

### D66 — `where`: Windows' `where.exe`, with dashes and cash's paths

`where.exe` is the Windows answer to "where is this file": it looks along the current
folder and PATH, tries the PATHEXT extensions on a bare name, takes wildcards, searches
a folder tree with `/r`, and reads `$VAR:PATTERN` and `DIR:PATTERN` for other folders.
cash had no such command; `which` answers what the shell would run, not where files are.
Asked for 2026-09-27, as a builtin with dashes for options and a help option.

- **Kept from `where.exe`**, measured against it on Windows 11: the search order (a
  folder's files in the order Windows lists them, the current folder before PATH, each
  folder once; with `-r`, a folder's files before those of the folders in it), the
  matching (any case, `*` and `?`, PATHEXT on a name without an extension, `NAME.*`
  matching `NAME`, folders never), a pattern given twice searched once, the `ERROR:` and
  `INFO:` messages on standard error, and the exit statuses: 0 when anything was found,
  1 when nothing was, 2 for a command line it cannot run.
- **Options** take dashes, decided with the user: `-r DIR`/`--recursive DIR`,
  `-q`/`--quiet`, `-f`/`--quote`, `-t`/`--times`, the letters in either case and bundled
  (`-qf`), and `-?`/`--help`. `/q` is no option, so a path is never taken for one; a
  pattern spelled like a `where.exe` option is searched for and, when not found, the
  message names the dash spelling.
- **Paths** print as `pwd` prints them, `C:/Windows/notepad.exe` (D3), decided with the
  user over `where.exe`'s backslashes. PATH and `$VAR` values may be in either form, and
  `DIR` may be spelled `/c/Windows`.
- **`-t`** shows the date in the user's own short-date format from Windows' region
  settings and a 24-hour time, as `where.exe` does.

It shadows `System32\where.exe`, which `cash doctor` names among the deliberate
shadows; `enable -n where` or the full path reaches the Windows one.

### D67 — `ls`: icons, a colour per kind of file, a tree, and more of GNU's sorts

On Arch the user's `ls` was lsd, which draws an icon for each kind of file. cash's `ls` is
its own (D48), so the question was what lsd adds and whether `ls` could have it. Decided
with the user, 2026-09-27, from lsd 1.2.0's options:

- **Icons**, by option only: `--icons[=always|auto|never]`, bare `--icons` meaning `auto`,
  so a pipe or a file never gets them. The icons are lsd's Nerd Font theme, matched by
  name, then extension, then kind, lsd's order; its tables are generated into
  `ls_icon_table.rs` (Apache-2.0, `NOTICE`).
- **Only where they draw** (revised with the user, 2026-09-28): `--icons-theme=auto`, the
  default, shows them only when Windows Terminal draws the tab in a Nerd Font, and shows
  no icons otherwise; `--icons-theme=fancy` shows them regardless. No terminal can be
  asked its font, but Windows Terminal names the tab's profile in `WT_PROFILE_ID`, and
  the profile's font is in Terminal's settings, resolved as Terminal layers them: the
  user's entry for the profile, `profiles.defaults`, the fragment that brings it, then
  Cascadia Mono (`cash_win32::terminal`). A Nerd Font by name (`Nerd Font`, `NF`, `NFM`,
  `NFP`) gets the glyphs; any other font, and anywhere cash cannot tell (the old
  console, VS Code's terminal, SSH), gets none. lsd's colour-emoji set
  (`--icons-theme=unicode`, briefly the fallback) was dropped as too loud: Cascadia Mono
  has no one-colour folder or file pictures, only shapes. Terminal ships no Nerd Font,
  so the profile `cash --terminal-profile` writes asks for one the machine has (D38).
- **A colour per kind of file**, as GNU ls gives with `LS_COLORS`: the shell's own
  `LS_COLORS` when set, otherwise `dircolors`' defaults (uutils' copy), which colour
  archives, backups and temporary files by extension as well as folders, links and
  programs. Parsing is the `lscolors` crate's; a program is known by its extension or a
  `#!` line, as before.
- **`--tree`** with **`--depth=NUM`** (lsd's), which also limits `-R`; links to folders
  are shown and not followed.
- **`--group-directories-first`**, and GNU's **`-X`**, **`-v`**, **`-U`** and
  **`--sort=WORD`**, which cash's `ls` lacked.

Two fixes came with it: whether output is a terminal is now asked of the command's own
standard output, so `ls | grep` inside an interactive cash gets one name a line and no
colour, and grid columns are measured in screen cells, so icons and non-ASCII names line
up.

Compared with lsd's `-la`, cash's `ls -l` then showed Unix bits it had made up, and more
files. Decided next, the same day:

- **`w` is the access list's answer.** `-l` asks Windows, with `AccessCheck` and the
  process's own token, whether this user may write the file's data (for a folder,
  create a file in it), from the same read of the security descriptor that finds the
  owner. A file's read-only attribute still clears it; a folder's does not, since
  Windows sets it only to mark a customised folder (`Contacts`, `Music`). The owner and
  group positions carry the answer, others stays read-only, and `x` is as before.
- **`--attributes`** shows lsd's letters, `d` or `.` then archive, read-only, hidden and
  system (`.a-h-`): a column after the permissions with `-l`, before each name
  otherwise.
- **Hidden and system together are left out** without `-a` or `-A`, as Explorer and
  lsd leave them out: `NTUSER.DAT`'s logs, and the `My Documents` and `Cookies`
  junctions kept for old programs. Hidden alone is still listed, as a dotfile is not.
  A name given on the command line is always listed.

And `ls` got faster: the owner, link count and permissions, which cost a read of each
file's security, are looked up only for `-l`, and an entry's size, times and attributes
come from the directory listing instead of opening it twice. Plain `ls -a` of
`C:/Windows/System32` went from 1.4 s to 12 ms, and `ls -la` from 1.45 s to 1.04 s with
the access check included.

Last, the user preferred the long format's colours as lsd (on Kali) draws them: with colour
on, `-l` now colours every column in lsd's theme, not only the name, as GNU ls does. The
permission letters one by one (the type blue, `r` yellow, `w` red, `x` green, `-` grey);
owner and group pale yellow when they are this user and grey for any other account, so
`Administrators` or `SYSTEM` stands out; the size by magnitude; the date by age, bright
green within the hour. Each column is padded before it is coloured, and `--color=never`
or a pipe leaves it all plain. Names still take `LS_COLORS`.

### D68 — The console is put back before each prompt

After `k3d cluster create` in a cash tab, letters still appeared but Enter and Backspace
did nothing, until the tab was closed. k3d had turned on `ENABLE_VIRTUAL_TERMINAL_INPUT`
and exited without turning it off; the console then hands over Enter as `\r` and
Backspace as `\x7f` instead of as keys, and the line editor reads keys. Bash on Linux
has the same exposure to a program that leaves the terminal in raw mode, and `reset`
(D55) was the only way back. Decided with the user, 2026-09-29: cash puts it right
before every prompt, silently (`cash_win32::console::repair_before_prompt`).

- **Console modes**, changed only where wrong: VT input off; processed output, wrapping
  and VT processing on; line feeds returning to the margin
  (`DISABLE_NEWLINE_AUTO_RETURN` off). The line editor's own raw mode is its business
  and is left to it.
- **The console's input settings as cash started** (added 2026-09-29, with the next
  three): window and mouse input, insert mode, QuickEdit and auto-position, remembered
  at startup (`remember_starting_modes`). A program that takes the mouse, as crossterm's
  and tcell's mouse capture do, turns QuickEdit off; a console with mouse input and no
  QuickEdit has Windows Terminal send the tab the mouse, so a plain drag no longer
  selects text, and later programs type over text instead of inserting. What the user
  set in the console's properties is kept, since it is the starting state that comes
  back.
- **The UTF-8 code page** (D41), which `chcp` or a program may have changed.
- **The terminal**, only when a program ran since the last prompt, since no program can
  ask the terminal what it left on. First the screen: the main screen rather than the
  alternate one a full-screen program died in, and scrolling over the whole screen
  rather than a region left behind, both between saving the cursor and restoring it.
  Bare, both move the cursor: leaving the alternate screen restores the cursor saved
  on entering it, which on a healthy screen is a stale one, and on a ConPTY the output
  landed back in the line before; setting the region moves it to the top. Terminals
  keep a saved cursor per screen, so after a full-screen program the restore puts the
  prompt where it was before the program started. Then attributes reset, cursor shown,
  mouse tracking and focus reporting off, cursor keys and keypad in their normal modes,
  the cursor shape back to the profile's, lines wrapping at the edge, and the ASCII
  character set, since a curses program that dies while drawing boxes leaves letters
  showing as line pieces. Colours and the palette are not reset: a program may have
  set them on purpose.
- **Keys typed ahead** while a program had VT input on reach the console as VT text
  (key-downs with no virtual key, `\r` for Enter, `\x7f` for Backspace, `ESC [ A` for
  Up), and stay that way after the repair. The line editor decodes them as crossterm
  decodes a terminal's bytes on Unix (`crates/cash-crossterm/README.md`, change 2);
  before, the command typed ahead needed a second Enter, and Backspace and the arrows
  typed `\x7f`, `[` and `A`.

The `conpty_a_program_that_*` tests in `conpty_interactive_tests.rs` break each of these
from a PowerShell child and check the prompt after it.

### D69 — `cash --init-rc`: a starter `~/.bashrc` for a user with no startup file

With neither `~/.bashrc` nor `~/.cashrc` (D24), a new user got bash's bare defaults: a `$ `
prompt, 500 lines of history, no aliases. `cash --init-rc` writes a starter `~/.bashrc`
then, and only then: an existing file of either name is left alone, and nothing is ever
overwritten. Decided with the user on 2026-09-29:

- **When: Scoop's install.** The manifest's `post_install` runs `cash --init-rc --once`.
  Scoop runs `post_install` after every update too, so `--once` acts only the first time
  for a user, as the marker `%LOCALAPPDATA%\cash\init-rc` records: a starter deleted on
  purpose does not come back with the next update. It prints only when it writes. Run by
  hand, without `--once`, it writes one whenever there is none; a zip install has that.
- **Which file: `~/.bashrc`.** Git Bash reads it too, so the starter works in both: its
  sections for cash alone check `$CASH_VERSION`, which Git Bash does not set. A Git Bash
  user's own `.bashrc` counts as having a startup file and is kept.
- **What: the author's own `~/.bashrc`, nearly as it is** (`crates/cash/src/starter.bashrc`,
  compiled into `cash.exe`): history large and shared between windows, the usual aliases,
  a prompt that shows the folder and the git branch (read from `.git/HEAD`, with no `git`
  process) and sends Windows Terminal's shell-integration marks, `ls --icons` (a Nerd
  Font draws them; Scoop's notes suggest one), and `coolfetch` once per window. Its
  comments stay, as the way to adapt it. Left out: the Alt-E folder picker, which needs
  broot and a hand-made broot configuration.

The Scoop bucket's manifest takes the `post_install` line only with the first release
that has `--init-rc`: an older `cash.exe` would take the flag for a shell option.

### D73 — `croot`: a file and folder picker on Alt-E

**Status: approved by the user on 2026-10-05.**

From 2026-09-27 the user ran broot on Alt-E as a "quick cd" (a `bind -x` in `~/.bashrc`
and a broot config of its own). After two weeks they want it in cash, as a picker
only: broot takes up to two seconds to open, being a separate program that loads its
config and builds its tree, and it does far more than pick a path. Built in, the picker
is drawn by the shell that is already running, from one folder read. Every choice
below is the user's, made by pick lists on 2026-10-05 (DONE.md phase 20).

**Where and how it draws.** Inline, below the command line, which stays visible with
the scrollback above it, as fzf's `--height` does: 40% of the window, at least 8 rows,
`CASH_PICKER_HEIGHT` as a percentage or a row count. Near the bottom of the window the
screen scrolls up to make room; a window too small for 8 rows gets the picker full
screen. Closing it erases its rows and leaves the prompt where it was. Keyboard only.

**What it shows.** broot's tree: the root on the first line, several levels open at
once, trimmed to fit with `… N more` lines, folders before files. Right makes the
selected folder the root, Left makes the root's parent the root; Left at a drive's
root shows the drives (each local and mapped drive, and `~`). Hidden entries (a name
starting with `.`, or the hidden attribute) and entries a `.gitignore` excludes are left
out unless Alt-. or Alt-I shows them. Alt-H swaps the tree for the folder history
(`cdh`'s list, most recent first). Alt-S orders each folder's entries newest first,
folders still before files, each with its age after it (`3h`, `2d`), and again by name
(the user, 2026-10-06). The root's own entries are always all listed and the list
scrolls through them; only deeper levels are trimmed (2026-10-05). A resized window gets
the picker's height worked out again. Entries take `ls`'s colours
(`LS_COLORS`); the picker's own parts (selection, matched letters, frame, status line)
take `CASH_PICKER_COLORS`, `name=SGR` pairs as `LS_COLORS` has them
(`sel=1;37;44:match=1;33:frame=2`).

**Typing** is a fuzzy filter on names (`crt` matches `crates`), best matches first. It
covers the open levels at once, and the levels below them in the background, capped in
time and entries so a large folder never slows the keys; a status line says when a
search is still running or was cut short. Backspace edits the filter, Esc clears it,
and Esc on an empty filter closes the picker without changing the line.

**Context from the command line.** The picker reads the line it was opened from:

- the command: folders only for an empty line, `cd`, `pushd`, `rmdir` and `mkdir`;
  folders and files for any other command and for an unknown one; Alt-F switches;
- the word under the cursor, when it is a path or part of one: the picker starts in
  its folder with its last part in the filter (`cd ~/src/fo` starts in `~/src`,
  filtered by `fo`), and the pick replaces that word; otherwise it starts in the
  current folder and the pick is inserted at the cursor.

**Picking** (Enter):

- an empty line, `cd` or `pushd`: the pick goes on the line and the line runs at once;
  an empty line becomes `cd PICK`;
- any other command: the pick is inserted with a space after it, and nothing runs;
- a command that takes several paths (`cp`, `mv`, `diff`, `ln`, `tar`, … and any
  command cash does not know) keeps the picker open for the next one, with the line
  above showing what was picked so far; a command known to take one (`source`, `.`,
  `cd`, `pushd`) closes it. Esc closes it; Ctrl-Enter picks and closes.

A pick is written in the shortest sensible form: relative to the current folder when
it is below it (`src/lib/`), `~/…` under the home folder, else in cash's `C:/…`
spelling (D3). It is quoted only when it needs quoting, and a folder ends in `/`.

**Keys.** Alt-E runs the Readline function `cash-picker`, which `bind` can move or
copy; cash's default bindings give it Alt-E, which replaces the user's broot binding.

**The builtin.** `croot [-d|-f] [DIR]` opens the same picker on the terminal, starting
in DIR, folders only with `-d` or files and folders with `-f`, and prints the
pick(s), one per line, on standard output: `cd "$(croot)"`, `vim $(croot -f)`. It exits
1 when closed without a pick, and 2 without a terminal.

**Speed.** The first frame comes from one read of the starting folder and must appear
within 30 ms of the key on a warm cache; deeper levels and the background search never
hold up drawing or keys. Measured on 2026-10-06 with `crates/cash/tests/croot_latency.rs`
(the dist build, a ConPTY, from writing Alt-E until the header came back): a median of
18.6 ms (worst 24.7 ms) in a folder of 44 folders and 16.9 ms (worst 21.5 ms) in one of
2,000 entries, against 1.6 ms for a plain key. A frame writes only the lines that
changed, as one synchronized update. Fuzzy matching and `.gitignore` rules come from established
crates (`nucleo-matcher` and `ignore`, or equivalents), not hand-written code.

Not in it: a search language, file previews, file operations, editor integration,
mouse input.

### D74 — Console, clipboard and the remaining small tools

**Status: asked for by the user on 2026-10-06** (DONE.md phase 21), after a second look at
what a clean Windows machine with cash still lacks. The first look
([the BusyBox gap analysis](research/busybox-gap-analysis.md), 2026-09-25) adopted the
process tools, `getopt`, `rev`, `clear`, `reset`, `bc` and `ping`, and left its tier 2
open. Of that tier, the tools below were chosen by two tests: nothing answers the name on
a bare Windows install with cash, and the tool has to agree with something cash owns, or
is cheap and collides with nothing. None of these names belongs to a program in
System32, so none hides one. Several (`tput`, `stty`, `iconv`, `column`, `xxd`, `nice`)
are also in Git for Windows' `usr/bin` and in Scoop shims; `type -a` lists those behind
the builtin, and the path still runs them.

**What cash owns, and the tool that has to agree with it.**

- **Console modes**: `tput` writes the VT sequences ncurses 6.6 writes for
  `xterm-256color`, as `clear` and `reset` already do, and takes `cols` and `lines` from
  the console even when standard output is a pipe. `stty` reads and sets the console's
  input and output modes with GNU coreutils' words (`-echo`, `raw`, `sane`, `size`, `-a`,
  `-g`); settings the console has no counterpart for are accepted and remembered so a
  script written for a tty runs; `-F` is refused. Cash puts the console back before each
  prompt (D68), so a change lasts for the current command or script.
- **Encoding**: `iconv` with glibc's options over every code page Windows has
  (`MultiByteToWideChar`), plus UTF-8, UTF-16 and UTF-32 with glibc's BOM rules and its
  "illegal input sequence at position N". `//TRANSLIT` uses Windows' best-fit mappings,
  which are not glibc's.
- **The clipboard**: `pbcopy` and `pbpaste`, macOS's names, which collide with nothing.
  `clip.exe` writes the console code page, so UTF-8 through it becomes mojibake, and
  Windows has no paste command. `pbcopy` stores Unicode text and turns lone LF into CRLF,
  so Windows programs paste it right; `pbpaste` writes UTF-8 and turns CRLF into LF, so
  `pbcopy < f; pbpaste | diff f -` is quiet for an LF file. A clipboard without text
  gives nothing, status 0.
- **File descriptors**: `flock`'s `FD` form (`exec 9>lock; flock -n 9`) only works
  through the shell's own descriptor table (D26), which no external program can see. The
  lock is `LockFileEx` on one byte far past the end of the file, so readers of the lock
  file are not blocked; it lives on the shell's handle and ends when the descriptor
  closes or the command ends. A directory is refused: Windows cannot lock one.
- **Process ids and command resolution**: `nice` and `renice` map niceness to the six
  priority classes (`-20…-11` HIGH, `-10…-1` ABOVE_NORMAL, `0` NORMAL, `1…10`
  BELOW_NORMAL, `11…19` IDLE; REALTIME is never set) and read the mapping back for bare
  `nice`; `renice -g` is refused, there being no process groups. `watch` runs its
  command through cash each time (procps runs `sh -c`, which is cash anyway).

**Cheap and colliding with nothing.** `column` (util-linux), `xxd` (vim's), `hexdump`
(util-linux, with its format language), `uuidgen` (util-linux), `free` (procps; `Mem:`
from the call `top` uses, `Swap:` the page file; no `Commit:` row, since scripts parse
`free` by its two row names), `xdg-open` (`start` under the name cross-platform scripts
try first, with xdg-open's exit codes), and `nc` (OpenBSD netcat's flags, Debian's
default; `-e` and `-c` refused, as OpenBSD refuses them).

**Oracles.** Where a Linux original exists it is the oracle: scripts in
`crates/cash/tests/oracle` run under the real tool in WSL (util-linux 2.42.3, procps-ng
4.0.7, coreutils 9.11, ncurses 6.6, glibc 2.44, vim's xxd) to make the golden files, and
under cash in the tests, with each deliberate difference replaced in the test beside its
reason, as `rev` and `getopt` are checked. The deliberate differences are the CRLF rule
(a CRLF line stays CRLF, D20) and the Windows mappings named above.

Not in it: `grep`, `diff`, `cmp` (the standing rule, README), the compressors, `patch`,
`strings` (Sysinternals' has the name and other flags), and the BusyBox applets the gap
analysis lists as not applicable.

### D75 — A per-user installer, `cash --update`, and winget

**Status: chosen by the user on 2026-10-06**, by pick list (DONE.md phase 22). The aim is
that installing cash gives a complete native Bash with the tools scripts need, without
Scoop. Scoop remains a channel (D38, D65, the packaging evaluation); the installer is the
second, on the releases page; winget is made from the installer.

- **Inno Setup, per-user**, no UAC (`PrivilegesRequired=lowest`), built on the GitHub
  Windows runner, attached to every release beside the zip. Silent switches for scripts
  and winget: `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART /DIR= /TASKS=`. Chosen over
  Velopack (its own update agent and layout, a .NET packaging tool) and a per-user MSI
  (Restart Manager against a running shell, ICE warnings, the most authoring).
- **Scoop's layout**: `%LOCALAPPDATA%\Programs\cash\<version>\` with a `current` junction,
  so an upgrade never overwrites a running `cash.exe` (D6 keeps the shell alive as long as
  its windows): open windows keep the old file, new tabs get the new one. Inno never
  closes applications. Every step after the copy is cash's own command
  (`--install-finish`: the junction, the Terminal profile, the tool links, `--init-rc
  --once`, sweeping old folders), so the Scoop and installer channels share one tested
  implementation, as D65 asked for PATH.
- **Tool links on PATH by default**, with a task to turn them off: `ls.exe`, `sed.exe`,
  `awk.exe` and the rest become programs on the user PATH, ahead of Git for Windows'.
  That is the "no Scoop needed" promise; the consequence, that PowerShell and editors get
  cash's tools too, is the point. cash writes PATH itself, as D65 has it.
- **Upgrades on request, never automatic**: `cash --update` fetches the latest release,
  verifies its `.sha256`, unpacks into a new version folder and moves the junction;
  `--check` only reports. A Scoop-installed cash points at `scoop update cash`. Background
  polling and silent self-replacement were turned down: a shell that changes itself
  overnight surprises script authors.
- **Uninstall** from Apps & features: links, Terminal profile, PATH entries and the folder
  go; `~/.bashrc`, history and config stay.
- **winget** from the installer (`InstallerType: inno`, `Scope: user`), the first version
  by hand, later ones by `winget-releaser`; after the installer has shipped in a release,
  since the installer type cannot change later.
- **Unsigned** (the user, 2026-10-06: no paid certificate, SignPath not wanted): a
  browser download of the setup gets SmartScreen's "unknown publisher" prompt once;
  winget and Scoop downloads do not. Said in `help installing` and the README.
- **Scoop offered, never imposed** (the user, 2026-10-06): the setup has an unticked
  task that runs Scoop's own installer as its last step, for the people who want the
  rest of their command-line tools the easy way; cash itself never bundles or runs a
  package manager otherwise. The installer refuses to run beside a Scoop-installed cash.
- **A hint when a command is missing**, at an interactive prompt only: Bash's message,
  then one line naming `winget install ID` and `scoop install NAME` from a curated table
  of common tools (`help tools`). Scripts get Bash's message and 127, nothing more, so
  no output changes for them.
- **The zip is the third channel, portable** (the user, 2026-10-06): one executable,
  unpacked anywhere, is a complete shell; the notices travel in the zip beside it. A
  portable cash offers once, at its first interactive prompt, to put itself on the user
  PATH, to add the Terminal profile and to put its tools on PATH, remembers the answers,
  and changes nothing without a yes; never for
  `-c`, scripts or a non-interactive shell. The user chose the offer over silence,
  knowing a shell started interactively by a script sees the question once.
- **`cash doctor` reports when cash itself is on no PATH**, with `cash --add-to-path`
  (and `--remove-from-path`) for the exe's own folder, the writer the offer shares.
- **F1, a one-screen help** for Windows users who know Bash (the user, 2026-10-06, by
  pick list): paths (six examples, a reason each), keys (seven), and one block on
  scripts, the built-in tools, `help`, `cash doctor`, `start` and `sudo`; in the prompt's
  colours, on the alternate screen like `less`, closed by F1, Esc or `q`; a Readline
  function `bind` can move. The starter `~/.bashrc` says `cash: F1 for help` once per
  window after the banner.

### D76 — `grep`, `diff` and `cmp` are carried after all

**Status: chosen by the user on 2026-10-06**, by pick list, reversing D35's and the gap
analysis's rule that cash carries no `grep` or `diff`. That rule was made for a cash
beside Git Bash; under D75's aim, install cash and have everything, `grep` is the single
most common failure on a bare machine and `diff` the second.

- **`grep`, `egrep`, `fgrep`** with GNU grep's interface, messages and exit codes, built on
  ripgrep's library crates (`grep-regex`, `grep-searcher`, `grep-printer`,
  `grep-matcher`; BurntSushi, MIT or Unlicense) as dependencies: ripgrep's engine behind
  GNU's options. BRE is the default and is translated to the regex crate's syntax; ERE
  with `-E`, fixed strings with `-F`; a backreference, which that engine lacks, runs the
  pattern through `fancy-regex` behind the same matcher trait, so nothing is refused.
  `-P` is refused by name. A CRLF line's `\r` is not part of the line (D20).
- **`diff` and `cmp`** from uutils diffutils (MIT), bundled like `sed` (D48), checked
  byte for byte against GNU diffutils, since `diff` output is what test scripts compare.
- Doctor's expectations for grep and diff become carried tools; `type -a` lists the
  Microsoft Coreutils and Git for Windows copies behind the builtins, as for every
  shadowed tool; `enable -n grep` reaches them.

### D78 — Archive and compression tools, on shared parts

**Status: chosen by the user on 2026-10-07**, by pick lists (DONE.md phases 26 to 30;
the design is `research/archive-tools-design.md`). Reopens D77's "xz and bzip2 stay with
Windows' `tar.exe`". Asked for first as `tar` alone; the user then asked for "a common
thing" for tar, zip and the rest, designed before anything is built.

- **The family**: `bzip2`, `xz` and `zstd` with their aliases, `tar` (GNU tar 1.35), and
  `zip`, `unzip`, `zipinfo` (Info-ZIP), each with its original's interface, messages and
  exit codes, oracle-tested in WSL. `tar` shadows Windows' `tar.exe` (bsdtar 3.8.8),
  which stays reachable by its path and by `enable -n tar`; doctor names it. Not now: the
  z-tools, `cpio`, `lzip`.
- **Two new library crates that know no shell and print nothing**: `cash-archive`
  (codecs, the archive member, walking the disk, names both ways, safe extraction,
  matching, listings, and the tar, zip formats) and `cash-getopt` (GNU option parsing,
  once). Front ends in cash-builtins turn their facts and typed problems into each
  tool's words. cash-win32 gains one Unix face for a Windows file and the link, time,
  attribute and name helpers it lacked; cash-core's `Pattern` gains path flags;
  cash-sed gains one public `s///`.
- **Moved onto the shared parts, under their own tests**: gzip; `ls` and `stat` (one
  rule for a file's mode, so `stat`'s answers change for some files, and the group is
  read rather than the owner repeated); `dos2unix`; the eleven hand-written option
  parsers and diffutils' own.
- **Every compression in pure Rust**: gzip (`flate2`), bzip2 (`bzip2` on
  `libbz2-rs-sys`, Trifecta Tech's port, under the bzip2-1.0.6 license, accepted for
  it), xz, lzma and lzip (`lzma-rust2`, a port of XZ for Java), zstd (`ruzstd`, which
  writes only at its fast level). The `zip` crate is built on the same four.
- Passed over: uutils/tar, at 0.0.1 less than `tar.exe` already does; `libzstd-rs-sys`,
  a prerelease without a Rust API, to replace `ruzstd` once it has one; `xz2` and
  `liblzma`, which build liblzma's C.
- **As built** (phases 28 and 29, 2026-10-07): each compressor keeps its own flow on
  shared blocks rather than one driver with profiles; tar writes and reads its own
  headers rather than the `tar` crate's; its walk, names and extraction stay in its
  front end until zip shares them; cash-core's `Pattern` and cash-sed are unchanged.
  The reasons are in the design's 3.11 and 3.12.
- **As built, zip** (phase 30, 2026-10-07): its own records rather than the `zip`
  crate's, for zipinfo's every field and zip's own header values; deflate on
  `miniz_oxide`, the RID as the numeric owner and "made on Unix", chosen in the phase
  without a pick list because the user asked for phase 30 to be finished without
  stopping (design 3.13), each open to change.

### D77 — `gzip`, `gunzip` and `zcat`, and the compatibility corners

**Status: chosen by the user on 2026-10-06**, by pick list (DONE.md phases 24 and 25).

- **The gzip family only**, pure Rust (`flate2`/`miniz_oxide`), with GNU gzip's
  interface and messages, oracle-tested; `xz` and `bzip2` stay with Windows' `tar.exe`,
  which reads their archives. Reopens the gap analysis's Q3 for gzip alone: a bare `.gz`
  has no reader on a bare machine, and even this tooled one has no `gunzip` or `zcat`.
- **Compatibility corners next**: `/dev/tcp` and `/dev/udp` redirections as Bash has
  them; `command_not_found_handle` run as Bash runs it, the install hint only without
  one; abbreviations kept across sessions in `%APPDATA%\cash\abbreviations`, as fish
  keeps its; kinder refusals for `suspend`, `mkfifo` and `stdbuf`; `getconf` and `locale`
  with the values Windows has. Chosen ahead of a signing key for `--update`, the
  adoption work (GIFs, a docs site, issue templates, `doctor --report`) and an ARM64
  build, which stay on the list.

### D79 — Ctrl-R: a history picker

**Status: chosen by the user on 2026-10-10**, by pick list (DONE.md phase 34;
`research/prompt-history-and-jump-design.md`).

Ctrl-R opens a list of the history below the command line, drawn as Alt-E's picker
(D73): its height, colours and moving keys, inline, Esc on an empty filter closing it
and leaving the line. Each command shows once, at its last use, newest first, with its
age; a command of several lines shows on one. Typing is the same fuzzy filter, on the
whole command, best matches first and the newest of equals first; what was on the line
when Ctrl-R was pressed is the filter it opens with. Enter puts the command on the line
to edit; Tab runs it at once. Ctrl-R in the picker switches between the whole history
and what ran in the current folder: cash records each command's folder in a file of its
own, `%LOCALAPPDATA%\cash\history-folders`, beside Bash's history file, which holds
commands and times only; commands from before that record show in the whole list only.
Bash's reverse search stays a Readline function `bind` can put back on a key.

### D80 — `z`: the folder jump

**Status: chosen by the user on 2026-10-10**, by pick list (DONE.md phase 34;
`research/prompt-history-and-jump-design.md`).

Every change of folder counts a visit to the folder arrived in, kept across sessions in
`%LOCALAPPDATA%\cash\folders` (this machine's paths, so local, not roaming). A folder's
rank is zoxide's: its visits weighted by the last one's age (×4 within the hour, ×2
the day, ×½ the week, ×¼ older); when visits add up past 10,000 all are scaled by 0.9
and those under 1 forgotten; a folder that is gone is skipped, and forgotten once it has
gone 90 days without a visit, as zoxide forgets one, so an unplugged drive keeps its
folders. `z foo bar` goes to the best folder whose path holds `foo` and then `bar`, the
last word in its last part, case aside, the current folder left out; a word that is a
folder is `cd`'s; `z` alone goes home and `z -` back; no match is status 1. `z -l`
lists the matches with their scores, `z -i` opens a list of them below the line, and
Tab after `z WORDS` offers them, best first, in place of the words. A script's `z`
reads the record and adds nothing to it. A `z` function, zoxide's included, comes
before the builtin. Alt-E's Alt-H (and `croot`'s) shows this record, best first, in
place of the session's 25 folders while it holds any; Alt-←/→ stay the session's back
and forward.

### D72 — `help` from one catalogue

Every builtin has an entry: a kind and a one-line summary, in
`crates/cash-builtins/src/helpdocs/builtins.md`. A test fails when one is missing, or when
an entry names no builtin. Pages and topics are markdown embedded at build time. A page
never copies options: it ends with the builtin's own `--help`, and for a bundled tool that
is the tool's real text. `help` marks the builtins that hide a Windows program in
System32, checked at run time. `cash help ...` is that builtin, run from outside the shell,
and gives way to a file named `help` in the working directory, as `cash doctor` does
(2026-10-04).

Help, and everything else cash prints, speaks to the user, never about cash's own
documents: no decision numbers, §4 rows, `spec.md`, ROADMAP items or research files
(the user, 2026-10-05). Where more reading helps, it names another `help` page.

### D71 — A program whose reader went away ends as SIGPIPE ends it, if it is cash's

On Unix, a program that writes to a pipe whose reader has gone is killed by SIGPIPE: it
ends at once, says nothing, and `$?` and `PIPESTATUS` show 141 (`yes | head -1` gives
`141 0`). Windows has no SIGPIPE: the write fails, and the program decides for itself
what to say and how to end. Decided with the user on 2026-10-03:

- **cash's own end as on Unix.** A builtin whose reader went away ends the shell it runs
  in with 141: a pipeline stage, a subshell, or a script, as SIGPIPE ends Bash's process;
  it only failed, so `while :; do echo y; done | head -1` went on for ever. An interactive
  shell goes on. A bundled tool (D48)
  runs in a child cash, and when its standard output is a pipe, it writes into a pipe of
  that child's own, whose contents a thread passes on; when the passing on finds the
  reader gone, the child ends there with 141, before the tool hears of it. So `seq`,
  `yes`, `cat`, `awk` and `sed` end in silence with 141, where they said `write error:
  Broken pipe` and ended with 0, 1 or 2. `env` and `timeout` are left out: they hand
  their standard output to the command they run, which would keep that pipe open.
- **Other programs keep their own ending.** cash cannot tell why a program it did not
  build ended, so `python -c 'print(…)' | head -1` ends as Python ends it (§4).

### D70 — Background jobs and subshells run inside the shell, on threads of their own

Bash forks a subshell for a background job, a compound or function pipeline stage, `( … )`
and a command substitution. cash forks nothing (D11): each is a copy of the shell running
on a thread of cash's own process. Decided with the user on 2026-10-03:

- **A thread of its own.** A background job and a compound or function stage run on a
  thread of the runtime's blocking pool. As tasks on the runtime's workers, a job that
  never waits (a loop of builtins) held its worker for good, and as many as cores left a
  foreground `$(…)` waiting for ever.
- **`$!` names every job.** A job that is not a program is given a number of its own:
  4n + 1, from 100001 up, which no Windows process can have, since their ids are
  multiples of 4. `$!`, `jobs -p`, `kill PID` and `wait PID` take it to mean the job, as
  Bash's take the forked subshell's pid. So is a job that begins with a compound command
  (`{ x=1; } &`, `while :; do :; done &`, `coproc { …; }`), at once: `&` does not wait
  for it to get anywhere, which deadlocked one that first read what the shell would write
  to it next. A job that begins with a simple command is waited for until that command
  has started a program, whose pid `$!` then is, or has ended.
- **`kill` ends a job running inside the shell.** It has no process to signal, so the
  signal is left where the job looks before each of its pipelines; the job ends there,
  with 128 plus the signal, and a program it waits for is signalled with the rest of its
  processes. A builtin that is still running, such as a `read` that waits, runs to its
  end first.
- **At most 256 at once.** Background jobs and compound or function stages running at
  the same time are limited to 256, what Git Bash's `ulimit -u` reports; one more fails
  with Bash's `fork: retry: Resource temporarily unavailable`. `CASH_MAX_SUBSHELLS` sets
  another limit, read when cash starts. The limit guards the blocking pool, which is made
  twice as large: a stage queued behind a full pool, waited for by the stages that hold
  it, would never start.

### D81 — `winglob`: at the prompt, a program of Windows' own globs for itself

**Status: chosen by the user on 2026-10-11**, by pick list (DONE.md phase 35).

`net user NAME * /add`, a line that works in cmd and PowerShell, fails in Git Bash and
failed in cash the same way: in a folder with files, `*` becomes their names, `net` is
handed `net user NAME AppData Documents … /add` and prints its syntax. Quoting the star
is the bash answer, and stays right in a script; at the prompt the line comes from a
Windows habit, as a pasted path does (D53).

The fix has one fact behind it: cmd never expands a wildcard, so every command-line
program Windows ships was written to read `*` and `?` itself when it wants them
(`xcopy`, `findstr`, `taskkill /im note*`, `forfiles /m`, `icacls`; OpenSSH's `scp`,
measured: a literal `*.txt` copied both files), or to mean something else by them (`net
user NAME *` asks for the password). The shell's globbing is never needed by such a
program and sometimes wrong: `xcopy *.txt d\` and `forfiles /m *.txt` fail as `net`
did. A program installed anywhere else, `python`, `node`, `rg` or `code`, needs the
shell's globbing, as it has it in Git Bash.

`shopt winglob`: when a simple command's name resolves, as the shell will resolve it to
run it, to a program under the Windows folder's `System32` or `SysWOW64` (at any depth,
so OpenSSH and `wbem` count; not the Windows folder itself, where `explorer.exe` and the
Python launcher's `py.exe` live), the words after it are not pathname-expanded. The other
expansions happen as ever, and `nullglob` and `failglob` have nothing to act on. A
builtin that runs the command among its words is looked through to it, each by its own
parsing (`Registration::command_operand`): `sudo`, so `sudo net user NAME *` works,
`command`, `exec`, `nohup`, `nice`, `env` and `timeout`. A function of the program's
name, and any other builtin, is served as bash serves them. The decision is made once
per command, before its second word is expanded, from the words expanded so far, so
`sudo $tool …` is looked through once `$tool` is known; the lookup is the one `hash`
caches.

Like `winpaths`, it is a divergence a script must not meet, and the same line can mean
different things typed and in a script: **on by default in interactive shells and off
otherwise**; `shopt -u winglob` in `.cashrc` turns it off at the prompt too, and
`shopt -s winglob` in a script turns it on there. The costs: `cmd /c echo *` prints `*`
at the prompt, as cmd itself would, and a Windows program that neither globs nor needs
to (`notepad *.txt`) gets the star it would get from cmd.

---

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
| 13 | A bundled builtin cannot delete the shell's current directory | It re-enters the binary as a child inheriting that cwd, and Windows refuses to delete a process's own cwd | D48 |
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
| 34 | `bc` is POSIX bc: GNU bc's language extensions are errors, each named, except a one-line `define`; after an error, `bc` exits 1 where GNU bc exits 0 | Decided scope (Q4); a named error beats a guessed extension, and a failed calculation should fail | D56 |
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

fails at the `rm`. A bundled builtin (D48) re-enters the binary as a child process and
inherits the shell's working directory, so this is a process deleting its own current
directory, which Windows refuses. bash-on-Windows succeeds because Cygwin emulates POSIX
unlink semantics.

It cannot be fixed by running the builtin in-process: re-entry is exactly what makes
redirections and pipes work for uutils. The workaround — `trap 'cd /; rm -rf "$d"' EXIT`
— is good practice anyway, and is asserted by a test so it stays working.

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
