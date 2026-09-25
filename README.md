# cash - Cool Again SHell

![cash logo](assets/cash_logo_small.png)

A bash-language shell whose execution model is **Win32**, hosted in Windows Terminal.

Avoided:

- a POSIX emulation layer
- a VM
- `msys-2.0.dll` underneath

The target is running ordinary bash scripts on Windows - Terraform wrappers, CI glue, pipelines with
`xargs`, `sed`, `grep`, `jq` - where the processes, paths and handles are all genuinely
native.

## Why bother

Windows already has Git Bash, MSYS2, WSL and Cygwin. They work by making Windows pretend
to be Unix. cash starts from the other end: keep the bash language, and make the
execution model native Windows.

## Design decisions worth knowing

Full rationale in [spec.md](spec.md); the short version:

| | |
| --- | --- |
| **Paths** | `C:/foo` is canonical. `C:\foo`, `/c/foo`, `/tmp`, `/dev/null` are all accepted; `pwd` always prints `C:/foo`. |
| **Arguments** | Never rewritten. No guessing which argv entries are paths — that is where MSYS2 needed `MSYS2_ARG_CONV_EXCL`. |
| **`PATH`** | The one translated variable: `:`-separated for scripts, `;`-separated for children. |
| **Ctrl-C** | Escalates. The grace period is the *second* Ctrl-C, not a timer, because `terraform apply` can legitimately take minutes to stop cleanly — and killing it holds the state lock. |
| **CRLF** | `\r\n` terminates a line wherever cash interprets line boundaries. Pipes between external programs stay byte-transparent. |
| **Exit codes** | Truncate like bash, except crashes map to `128 + n` — an access violation is `139`, just like a segfault. Naive truncation would report `0xC0000100` as success. |
| **Shell robustness** | Release builds unwind panics. An unexpected panic while processing an interactive command is reported, sets status 1, and returns to the prompt; startup and non-interactive panics stop safely with status 1 instead of aborting the process. |

cash's deliberate divergences from bash are documented in [spec.md](spec.md) §4.
That list is meant to stay short.

cash reports Bash 5.2.37 through `$BASH_VERSION`. It also supports selected
Bash 5.3 additions: `${ command; }` runs a substitution in the current shell,
preserving changes to shell variables, while `${| command; }` expands a temporary
`REPLY` and leaves command output on stdout. `compgen -V name` stores candidates
in the indexed array `name` instead of printing them; no matches clear the array
and return status 1. `source -p PATH file` and `. -p PATH file` use an explicit
source search path. These additions do not imply full Bash 5.3 compatibility.
The [Bash source comparison](research/bash-reference/README.md) records the
behavioral probes and how to rerun them.
Known gaps in the advertised Bash 5.2 interface are tracked in the
[Bash 5.2 gap audit](research/bash-reference/bash-5.2-gaps.md).
The central [project roadmap](ROADMAP.md) keeps the feature order explicit: Bash 5.2,
native `awk` and `sed`, and the bundled userland (`stat`, `tty`, `install`, `pathchk`,
`nohup`, `who`, `users`, `pinky`, `logname`, `hostid`) are done; Bash 5.3 is in progress,
with every script-observable item probed against a real Bash 5.3 in the
[Bash 5.3 audit](research/bash-reference/bash-5.3-audit.md).
The selected source candidates and release gates are recorded in the
[`posixutils-rs` AWK evaluation](research/posixutils-rs-evaluation.md) and
[`uutils/sed` evaluation](research/uutils-sed-evaluation.md).

History and job-control scripts can use `history -n`/`-r`/`-p`, `fc` editor mode,
and `jobs -n`. `BASH_COMMAND` preserves the command text before expansion, assigning
`SECONDS` resets its signed live counter, and symbolic `umask` forms round-trip even on
Windows, where the mask is remembered rather than applied to ACL-based file creation.
Windows does not load Bash C-ABI builtin modules: `enable -f` and `enable -d` report
"dynamic loading not available" with Bash-compatible status 2.
Missing and deliberately limited command-line options are tracked separately in the
[builtin option audit](research/builtin-option-gaps.md), including cash's native Windows
`ps`, `pgrep`, `tree`, `top`, `find`, and `xargs` implementations.

Interactive commands are persisted in `~/.cash_history`. Removing that file does not
erase entries already loaded by a running shell; use `history -c` to clear memory and
`history -w` to replace the file with the cleared history. Cash's test harness gives
interactive cases an isolated temporary home, so test commands never enter user history.

Interactive `read -e` and `read -E` support editable `-i` text, cursor movement, insertion,
Backspace/Delete, Home/End, and the common Ctrl-A/E/U/K/W controls. `read -s` and
timeouts continue to apply while editing. `-E` is accepted with the same editor but does
not yet add Bash completion. This is a compact built-in editor; GNU Readline history,
completion, and custom `bind` keymaps are outside that surface. The remaining `read`
details and source-level comparisons are in the
[Bash 5.2 gap audit](research/bash-reference/bash-5.2-gaps.md). A source-level
[Rustyline evaluation](research/rustyline-evaluation.md) records why Cash uses it as a
source reference rather than adding another prompt backend or replacing the
descriptor-aware `read` editor.

## Layout

```
crates/cash              the binary
crates/cash-win32        the Win32 semantics layer — job objects, signals, paths, encoding
crates/cash-core         shell runtime: expansion, control flow, traps
crates/cash-parser       bash grammar — tokenizer and parser
crates/cash-builtins     the standard builtins
crates/cash-interactive  line editing, history, completion
crates/cash-shell        the shell library that crates/cash drives
xtask/                   test orchestration
spec.md                  the decisions, with the reasoning behind each
NOTICE                   brush attribution and the list of modifications
```

The `cash-core`, `cash-parser`, `cash-builtins`, `cash-interactive`, `cash-shell` and
`cash-test-harness` crates are **absorbed from brush** at upstream commit `737dd57` and
modified. Both projects are MIT, so this is a straightforward absorption rather than a
dependency; see [NOTICE](NOTICE) for attribution and the list of changes.

Nothing is upstreamed. The trade is a coherent codebase instead of a patched copy of
someone else's, at the cost of porting future upstream improvements by hand.

`cash-win32` stays a separate crate so the Windows semantics and the shell language
remain separable.

## Build

```bash
cargo build --release
cargo test -p cash-win32
```

Windows 11 (or Windows 10 1809+, for ConPTY). Rust 1.88+.

Source files use LF even on Windows. `.gitattributes` defines Git's line-ending
policy, and `.editorconfig` asks editors to save matching endings. Windows scripts
(`.bat`, `.cmd`, `.ps1`) use CRLF; byte-sensitive CRLF/BOM fixtures are exempt from
normalization. No global `core.autocrlf` change is needed.

Git's line-ending conversion warnings concern files on disk, not child-process
output. `xargs` prefers enabled cash builtins, so `xargs echo` uses cash's `echo`
regardless of which `echo.exe` is on `PATH`. Explicit executable paths bypass builtins.

## Userland

cash carries the whole of uutils coreutils, plus a set written for Windows because the
name resolves to something worse there: `find` and `xargs`, `ps`, `pgrep`, `pstree`, `tree` and
`top`, `less`/`more`,
`which`, `chmod`, `hostname`, and `coolfetch`.

The rule for what cash carries is not "is it missing" — Scoop can supply anything. It is
**does the tool have to agree with cash about something cash owns?** `find` prints paths,
so it owes D3 one spelling; `xargs` builds command lines, which on Windows are strings the
callee re-splits (D32); `ps` and `top` print pids that `kill` must accept (D22). Where the
answer is no, cash stays out of the way.

`top` uses the familiar procps overview with uptime, task and CPU summaries, physical
memory, and a live process table. Press `?` or `h` for its compact help; `P`, `M`, `T`,
and `N` sort by CPU, memory, accumulated CPU time, and PID, while the arrow and page keys
scroll. Enter, Space, or `r` refreshes immediately, and `q` or Escape exits. Batch mode
(`top -b`) remains plain text for pipes and logs. Sampling is local and cheap: process
CPU/memory plus machine CPU, uptime, memory, and one local PDH processor-queue counter.
The displayed load average is a session-local Windows analogue: busy CPU equivalents plus
ready threads, exponentially averaged over 1, 5, and 15 minutes. Windows does not preserve
that history before `top` starts, and the value does not include Linux's uninterruptible
sleep state. Swap, nice values, and the Linux buffer/cache split are omitted. `Commit`
shows Windows committed memory against its limit and peak. `PRI` is the
Windows base scheduling priority; on supported Windows releases, `SHR` is the shared part
of the resident working set. The `S` column means no CPU time accrued in the latest
sample; `R` means some did.

Process IDs use native Windows numbers throughout. `$$` and `$BASHPID` keep their Bash
spellings, `$PID` is the equivalent direct spelling familiar from Windows, and `$PPID`
is the native parent process ID. `ps -efj` adds parent and Windows base-priority columns;
`pstree [-p] [PID]` renders the native parent tree; and `pgrep -P PID` or
`pgrep --parent PID` selects native children. Their output can be passed directly to
cash's `kill`.

`fuser` and `lsof` answer "who holds this file" and "who owns this port" from the
Restart Manager and the socket tables: `fuser -k file.txt`, `kill $(lsof -t -i:8080)` and
`lsof +D build/` work, with the Windows limits listed in spec D50. `ss -tulpn`, `ss -ltn` and
`ss -tn state established` print iproute2's layout from the same socket tables (spec
D51), so port-wait loops written for Linux work unchanged.

`tree [DIRECTORY]` renders a sorted filesystem hierarchy without following directory
links or junctions. Its useful common options are built in: `-a`, `-d`, `-L LEVEL`,
`-f`, `--dirsfirst`, and `--noreport`.

**It now bundles native `awk` and `sed` implementations** (providing complete POSIX
pattern scanning, text stream transformation, in-place file editing, and script processing
without external dependencies). They keep Windows files intact: a CRLF line is matched
without its CR, so `$` works, and is written back as CRLF. A program that names `\r`
(`sed 's/\r$//'`) sees the real line, and `CASH_EOL=lf` (or `sed -b`) gives Linux behaviour
everywhere. `dos2unix` and `unix2dos` convert files between the two.
**It does not carry `grep` or `diff`** — they have good Windows builds and do not have
to agree with the shell:

```bash
winget install Microsoft.Coreutils   # coreutils + grep
```

Git for Windows supplies all of them too, and most people running bash scripts on Windows
already have it.

Run `cash doctor` to see what this machine has. It reports what **cash** would run, not
what is on `PATH` — a builtin is never reported as missing, and a DOS tool in System32 is
never reported as shadowing something cash carries. It names missing `sed`, BusyBox
applets masquerading as fuller implementations, DOS `find`/`sort` winning over the Unix
ones, and Store aliases whose target app is not installed. None of those announce
themselves.

`sh` and `bash` resolve to cash itself, ahead of `PATH`. On Windows the alternative is
`C:\WINDOWS\system32\bash.exe`, which is the WSL launcher: without this, a script
running `bash helper.sh` would silently continue under Linux. A real bash is still
reachable by full path.

Worth knowing: the answer depends on which shell launched cash, because it inherits that
shell's `PATH`.

## Licence

MIT. Vendored brush is MIT, © Reuben Olinsky.
