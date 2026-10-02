# To do

Everything found while working on cash that is not done yet, in one list, and the plan
for working through it.

## How this is worked

- **One session, one item at a time, in the order below.** No further worktrees and no
  spawned sessions: what is found on the way is added to the phase it belongs to, or to
  the end, not started beside it.
- **Per item:** reproduce it (the code has moved since the item was written down),
  compare with Git Bash 5.3, fix it with a test that fails without the fix, and commit.
  A done item is deleted here in the same commit.
- **Per phase:** `cargo fmt --all`, `cargo clippy --workspace --all-targets`, the full
  nextest run with `--retries 0`; then `main` is brought up to the branch and pushed,
  the phase is released once CI is green, and a short report is written. The next phase
  starts without waiting. The work stops only for a decision marked **yours** below, or
  for a release whose CI fails.
- **Releases:** each phase ends with a release of its own. A tag is made only after CI
  has passed on the commit (RELEASING.md). So far: 1.3.0 is what was on `main` on
  2026-09-30, 1.3.1 the test suite (phase 1), 1.3.2 Ctrl-C everywhere (phase 2), 1.3.3
  process substitution (phase 3), 1.3.4 the `/dev` names and the bundled tools (phase 4).
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before. All of
their work is in `main`; the sessions themselves are not needed any more. The order and
the grouping were chosen that day: the test suite first, so that every later phase can
trust its runs, then by what a user notices most.

| Phase | What | Items |
| --- | --- | --- |
| 5 | Jobs, `wait`, and the tools beside them | 5.2 to 5.4 |
| 6 | Updating cash | 6.1 |

---

## Phase 5. Jobs, `wait`, and the tools beside them

### 5.2 Small things left in `kill` and held processes

From the session that guarded `kill` against reused pids (spec D22). Each is done only if
it is cheap while 5.1 is in the job table; what is not moves to `open-issues.md`.

- Holding a job stopped with Ctrl-Z open has no test of its own: one line calling the
  tested `hold`, and a test needs the ConPTY harness.
- Still asked by number, and so open to a reused pid: a pid cash never started as a job's
  (a foreground command, one read from `ps`), and a job's process pushed out of the held
  set after 1024 later ones ended.

### 5.3 `fuser` and `lsof` are slow on a system DLL

Reported, with measurements. `fuser -v kernel32.dll` took 2.5 to 6.5 s alone and 41.8 s
beside another build. The Restart Manager refuses a system DLL at once, and
`file_holders` then walks every process: about 15,500 modules in 290 processes to list
(0.9 s idle, up to 14 s under load), and `path_key` opens each module's file to
canonicalize it (1.3 to 4.9 s). `crates/cash-builtins/src/fileuse.rs`.

### 5.4 `${!}` and `${!:+word}` are not expanded

Found in 5.1 (2026-10-02). `echo "[${!}]"` prints `[${!}]` and `echo "${!:+set}"` prints
`${!:+set}`, as written, where Git Bash 5.3.15 prints the pid and `set`. `$!` and `${#!}`
work. Likely the parser taking `${!` for the start of an indirection (`${!name}`).

---

## Phase 6. Updating cash

### 6.1 `scoop update cash` from within cash

Asked by the user on 2026-10-02. Updating from 1.1.5 to 1.2.1 on 2026-09-30, Scoop said:

```text
ERROR The following instances of "cash" are still running. Close them and try again.
 ...  49516   1 cash
Running process detected, skip updating.
```

The shell `scoop update cash` was typed into was Scoop's own cash
(`~\scoop\apps\cash\current\cash.exe`), so the update refused itself; it went through
later from elsewhere (`current` points at 1.2.1 since 2026-09-30 20:25). What is known:

- Scoop refuses to update an app while a process runs from the app's folder, and goes on
  with `scoop config ignore_running_processes true`. The README (Install) and the
  manifest's notes (`packaging/scoop/cash.json`) say so.
- Each version is installed in a folder of its own and `current` is a junction Scoop
  points at the new one, so a running cash holds only its own version's files. Not yet
  looked at: what Scoop does meanwhile with the old folder, `persist`, the shim and the
  Windows Terminal fragment the installer writes.
- The bucket's Excavator picks a release up within hours (every few hours, by schedule),
  so `scoop update` right after a release can still offer the one before.

Options to investigate, none decided:

- Setting `ignore_running_processes` for the user, at install or on request: a setting
  of the user's, so **yours** if it comes to that.
- A builtin or a command of cash's (`cash update`?) that runs the update from a process
  outside the app's folder, or once the shell that asked has exited.
- How other shells installed with Scoop deal with it (PowerShell 7, Nushell, Git's bash).
- Whether a manifest can tell Scoop which running processes to ignore.

---

## Decided, written down so it is not decided twice

- **Timing-sensitive tests run on an idle machine, and their failures under load are
  never fixed** (the user, 2026-10-02). They are run with the CPU free and nothing else
  of this project beside them: no second build or test run, no other session's. One
  that fails under load is run again on an idle machine and judged from that; no
  timeout is lengthened and no test rewritten for it. `open-issues.md` entry 10 lists
  the ones seen.
- **`> /dev/stdout` shares the descriptor, it does not open the file again** (spec D7,
  §4 row 39). Git Bash reopens, so `{ echo a; echo b > /dev/stdout; } > out` leaves `b`
  there and `a`, `b` in cash. Both `/dev/stdin` sessions chose sharing, on their own.
- **Two sessions fixed `/dev/stdin` at the same time.** `zealous-goldstine`'s
  implementation is the one in `main` (it was built on the `/dev/tty` work). Of
  `kind-nash`'s, the tests, `open-issues.md` entry 9 and the spec row were kept and its
  implementation was not; its commits are in the history behind the merge.
