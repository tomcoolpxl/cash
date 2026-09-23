# cash - Cool Again Shell

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
to be Unix. cash starts from the other end: keep the bash *language*, and make the
*execution model* Windows.

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

cash diverges from bash in twelve specific, documented places ([spec.md](spec.md) §4).
That list is meant to stay short.

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

## Userland

cash carries the whole of uutils coreutils, plus a set written for Windows because the
name resolves to something worse there: `find` and `xargs`, `ps` and `top`, `less`/`more`,
`which`, `chmod`, `hostname`, and `coolfetch`.

The rule for what cash carries is not "is it missing" — Scoop can supply anything. It is
**does the tool have to agree with cash about something cash owns?** `find` prints paths,
so it owes D3 one spelling; `xargs` builds command lines, which on Windows are strings the
callee re-splits (D32); `ps` and `top` print pids that `kill` must accept (D22). Where the
answer is no, cash stays out of the way.

**It does not carry `grep`, `sed`, `awk`, `diff` or `stat`** — none of them has to agree
with the shell about anything, and all of them have good Windows builds:

```bash
winget install Microsoft.Coreutils   # coreutils + grep
scoop install sed gawk               # NOT in the MS bundle — separate GNU projects
```

Git for Windows supplies all of them too, and most people running bash scripts on Windows
already have it.

Run `cash doctor` to see what this machine has. It reports what **cash** would run, not
what is on `PATH` — a builtin is never reported as missing, and a DOS tool in System32 is
never reported as shadowing something cash carries. It names missing `sed`/`awk`, BusyBox
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
