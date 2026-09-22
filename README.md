# cash — Cool Again Shell

A bash-language shell whose execution model is **Win32**, hosted in Windows Terminal.

Avoided:

- not a POSIX emulation layer
- not a VM
- no `msys-2.0.dll` underneath

The target is running ordinary bash scripts on Windows - Terraform wrappers, CI glue, pipelines with
`xargs`, `sed`, `grep`, `jq` - where the processes, paths and handles are all genuinely
native.

## Why bother

Windows already has Git Bash, MSYS2, WSL and Cygwin. They work by making Windows pretend
to be Unix. cash starts from the other end: keep the bash *language*, and make the
*execution model* Windows.

Two things make that worth doing now:

- **[brush](https://github.com/reubeno/brush)** is a bash-compatible shell in Rust with
  ~2,500 differential tests against real bash. The language is solved.
- **[Coreutils for Windows](https://github.com/microsoft/coreutils)** ships native Win32
  `cat`, `grep`, `find`, `xargs` and friends. Most of the userland is solved.

What was missing is the layer between them, which is what cash is.

## The thing Windows does better than Linux

On Linux, killing bash does **not** reliably kill what it started. Descendants reparent
to init and survive; cleanup depends on cooperative `SIGHUP`-to-process-group, which
`nohup` and `setsid` defeat.

Windows job objects are kernel-enforced. A process cannot leave its job, children join
automatically, and closing the job reaps every member.

Verified end to end: `cash(outer) → cash(inner) → ping`, then `TerminateProcess` on the
outer alone — no cleanup, no signal, nothing able to cooperate. Both descendants died.

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

## Recommended userland

cash requires no particular userland and never will. It works with whatever is on
`PATH`. That said, the combination it is designed against:

```bash
winget install Microsoft.Coreutils   # coreutils + find/xargs + grep
scoop install sed gawk               # NOT in the MS bundle — separate GNU projects
```

`cash doctor` reports what it found — missing `sed`/`awk`, BusyBox applets
shadowing fuller implementations, DOS `find`/`sort` winning over the Unix ones, Store
aliases whose target app is not installed. None of those announce themselves.

Worth knowing: the answer depends on which shell launched cash. On the machine this was
designed against, `cash doctor` reports fourteen warnings under PowerShell's `PATH` and
one under Git Bash's.

## Licence

MIT. Vendored brush is MIT, © Reuben Olinsky.

## Working on this

The absorbed language layer is large. Release builds of it take ~2 minutes; **debug builds take
~45 seconds**, and per-crate tests are near-instant:

```bash
cargo build -p cash                   # debug, for iterating
cargo test -p cash-parser             # 235 tests, 0.12s
cargo test -p cash-win32              # 72 tests
```

Reach for `--release` only when measuring performance or producing a binary to ship.
Iterating on a parser change with full release rebuilds wastes minutes per attempt.

When a parse is going wrong, trace the tokenizer rather than reasoning about it. Dropping
a temporary `eprintln!` into `consume_nested_construct` found a bug in one run that three
rounds of careful reading had missed — the token text carries the blank that preceded it,
so `" esac"` arrives rather than `"esac"`.
