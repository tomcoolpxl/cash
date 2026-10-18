# Bundled tools inside the shell, or as processes

The question (2026-10-16): `echo hi | cat` costs 19 ms in cash, 1.7 ms in WSL's bash, and a
`while read` loop piping into `cut` pays that for every line. Should the bundled tools run
inside the shell, as cash's own builtins (`grep`, `ls`, `tar`) do, rather than as a process
of cash.exe each? This note gathers what was found before anything changes: why they are
processes, what other shells do, what a process gives cash today, what running inside
would cost, and the bugs the investigation turned up on the way.

## Why they are processes

- **brush's design**, kept since the start (`crates/cash-shell/src/bundled.rs`, module
  doc): utilities "execute as a subprocess of cash so that shell redirections, pipes, and
  process-group state are honored by code that reads/writes the host process's standard
  fds (e.g., uutils crates)".
- **The user's decision of 2026-10-04** (`decisions.md`, "Bundled tools stay processes of
  their own"; recorded in 261f52fb): "a call costs a process start, about 25 to 35 ms in a
  debug build against 0.5 ms for pure shell, and a tool that crashes or exits cannot take
  the shell with it. Running them inside the shell's process was turned down." The same
  entry turned down a native `grep`, which was later reversed (grep is cash's own now).
- **Work built on the child since**: the `/dev/null`, `/dev/stdin` names patched into the
  tool's process "and never in the shell" (866b34b9), the command line patched so a tool
  names itself (907b5c9d), 141 on a broken pipe by ending the child (7d981a12, D71),
  `kill` through the job object (0fbb3dad), the `env`/`timeout` MSYS relay (26d1b6d0).

Third-party programs are not part of it: a program that runs `sed` itself (`make`, a Git
hook, Python's `subprocess`) starts whatever `sed.exe` is on `PATH`, Git's or a link of
`cash --link-tools`, and never sees cash's builtins. Moving a tool inside changes only what
cash does when a script names it.

## What other shells do

Every shell splits the same way: small, instant tools inside, the rest processes.

- **BusyBox** (`docs/nofork_noexec.txt`), the closest case, with every tool compiled into
  the shell's binary as in cash. A tool runs inside ("NOFORK") only if it never calls
  `exit`, frees everything it took, keeps no global state, and never runs long or waits for
  input, as Ctrl-C is handled only after it returns: "Applets which must be interruptible
  by ^C in shells can not be NOFORKs." Candidates are those "small and easy to audit" and
  run "particularly in shell script loops". `yes` was moved back out after Ctrl-C trouble.
  In its source: `basename` and `dirname` are NOFORK; `head`, `cut`, `seq`, `yes` run in a
  forked child (NOEXEC, a Unix trick Windows has no equivalent of); `cat`, `wc`, `tr`,
  `tee`, `sleep` are full processes.
- **ksh93** builds in `basename`, `cat`, `cut` and `dirname` by default, "commonly used in
  performance-sensitive code paths in scripts"; `head`, `wc`, `mkdir` and others were taken
  out of the defaults "for lack of convincing advantages".
- **bash** and **zsh** run all of them as programs (bash has loadable builtins, off by
  default).

## What a process gives cash today

- **Its own directory and environment**: the child starts in the shell's directory with the
  exported variables (`commands.rs:713-733`). cash never changes its own process directory
  (D10) and never writes shell variables into its process environment.
- **Ctrl-C, Ctrl-Z and kill**: a foreground child gets Ctrl-C itself; job objects end or
  suspend whole trees (D6, D13, D19, D21, D22); `kill $!` reaches a real pid.
- **`&` returns at once**, with `$!` a real pid.
- **`nice`** sets the child's priority class.
- **Crash isolation**: a panic, stack overflow, `abort`, `process::exit` or out-of-memory
  ends the child; the script goes on with its status (139 for a stack overflow).
- **Process-wide hooks** the tools need (`/dev` names, command line, standard handles)
  stay in the child.

## What running inside would cost

### uutils cannot run inside cash at all

uucore, shared by every uutils tool, keeps process-wide state (uucore 0.12.0):

- the tool's name is read once from the process command line (`lib.rs:373`): messages
  would say `cash.exe:`;
- one exit code for the process, never reset (`mods/error.rs:66`): after one failure every
  later tool reports failure, and concurrent tools overwrite each other's;
- messages: one tool per thread (`locale.rs:188-222`) and one tool's messages per process
  (`locale.rs:185-187`);
- locale and time zone read once from the process environment: `LC_ALL=C sort`,
  `TZ=UTC date` could not work;
- the panic hook is wrapped again on every run (`panic.rs:36-43`);
- errors and help go to the process's own stderr and stdout.

And tools that would end or cripple the shell: `cat`, `tail`, `tr` call `exit(13)` on a
broken pipe; `uniq --bogus` exits; `sort` installs a Ctrl-C handler that calls `exit(2)`
and swallows every console event; `tee -i` switches Ctrl-C off for good; cash's `sed` and
`awk` call `exit(141)`. A tool brought inside must be written for cash, as `grep` was.

### Even a native tool loses what the process gave

- **Ctrl-C** is acted on between commands only; a builtin stops early only if it polls
  `take_interrupt()`. A second Ctrl-C ends the whole shell (`console.rs:162-167`).
- **`&`** waits for a builtin started as a simple command to finish (see bug 1), and
  `kill`, `kill -STOP` and Ctrl-Z cannot reach it; `$!` is a job number, not a pid.
- **`nice`** has no effect; **`xargs -P`** loses parallelism for a tool that blocks.
- **A broken pipe** in a single command or a `lastpipe` stage ends the script
  (`commands.rs:1705-1720`), where the child would end alone.
- **Stack overflow, abort, out-of-memory** end the shell and, through the session job,
  everything it started.
- **Every path** must go through `shell.absolute_path` and every variable through the
  shell's environment, or relative operands resolve against the directory cash started in.

One thing gets better: `rm -rf "$d"` of the shell's own directory (spec §4 #13) would work,
as cash's process never sits in that directory. The spec's "cannot be fixed by running the
builtin in-process" (spec.md:411) is wrong.

## The tools, by what they would risk

| Kind | Tools | Inside? |
|---|---|---|
| Pure string work: no input, no files, instant | `basename`, `dirname` | Safe, as BusyBox has them |
| Filters that may read input forever (a terminal, a pipe) | `cat`, `head`, `tail`, `cut`, `tr`, `wc`, `tee`, `uniq`, `sed`, `awk`, `od`, the checksums | Only once builtins stop on Ctrl-C and `&` returns at once |
| Never-ending or waiting | `sleep`, `yes`, `tail -f`, `seq inf`, `ping`, `dd`, `more` | No: they need Ctrl-C and `kill` |
| Heavy work, memory, temp files, threads | `sort`, `shuf`, `split` | No |
| Run other programs | `env`, `timeout` | No |
| Change files, by relative paths | `cp`, `mv`, `rm`, `mkdir`, `ln`, `touch`, `mktemp`, … | Not worth the risk |

How often they appear in real scripts (tests/corpus and the oracle scripts): `dirname`
leads in real-world scripts, mostly in `$(…)`; then `cat`, `tr`, `wc`, `sort`, `head`,
`sed`, `cut`.

## Bugs found on the way

These stand whatever is decided.

1. **A builtin started with `&` blocks until it finishes.** `grep -r … &` returned after
   102 s; a bundled `sleep 2 &` after 21 ms. `&` waits for the job's pid, which a builtin
   publishes only when it ends (`interp.rs:628-638`, `commands.rs:1681-1693`). Every
   native builtin is affected: `grep -r`, `find`, `tar`, `zip`, `ls -R`.
2. **Two Ctrl-C end the shell during a long native builtin.** `grep`, `find`, `ls` and
   the archivers never poll for Ctrl-C, so the first press shows nothing and the second
   is left to Windows, which ends the process (by design, as an escape hatch).
3. **Parsing an `env` or `timeout` command line runs uucore's setup inside the shell**
   (`cash-coreutils-builtins/src/lib.rs:73-81,264-286`): each command wraps the global
   panic hook once more, fills uucore's message store with `env`'s, and can `exit(99)` the
   shell if that fails.
4. **`{ head -n1; cat; } < file` loses what `head` read ahead**: uutils' head seeks the
   shared input back only on Unix (`uu_head head.rs:460-489`).
5. **spec.md:406-412** says an in-process `rm` could not fix §4 #13; it could.
