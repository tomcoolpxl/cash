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
  process substitution (phase 3).
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before. All of
their work is in `main`; the sessions themselves are not needed any more. The order and
the grouping were chosen that day: the test suite first, so that every later phase can
trust its runs, then by what a user notices most.

| Phase | What | Items |
| --- | --- | --- |
| 4 | The `/dev` names, descriptors and the bundled tools | 4.1 to 4.6 |
| 5 | Jobs, `wait`, and the tools beside them | 5.1 to 5.3 |

---

## Phase 4. The `/dev` names, descriptors and the bundled tools

### 4.1 The `/dev` names as an argument of the bundled tools

Reported by both `/dev/stdin` sessions. `open-issues.md` entry 9, spec D7 and §4 row 39.

- `cat /dev/stdin`, `cat /dev/fd/0`, `tee /dev/stderr`, `cat /dev/null > f`,
  `cp /dev/null f`: "The system cannot find the path specified.", status 1.
- The names are known only where cash opens the file itself (a redirection, `source`).
  An argument reaches a command as written (D4); translating arguments for tools whose
  grammar cash does not know is what D28 was revoked for.

**Decided: the tools cash bundles accept the names themselves, all of them, in one shared
place.** Where a bundled tool opens a file, `/dev/stdin`, `/dev/stdout`, `/dev/stderr`,
`/dev/null` and `/dev/fd/N` are what they are in a redirection, so cash translates no
argument. 3.2 made a start: `cash_win32::pipe::open_output` is where `tee`, `sort`,
`uniq` and `shuf` open the file they write, patched in `vendor/uutils`. A program on `PATH` still gets the name as written and fails. This is a new
decision for the spec. **Yours, if it comes to it:** should the bundled tools turn out to
have no one layer their file opening goes through, the work stops with the list of tools
and what each would cost.

### 4.2 File tests on the `/dev` names are false

Reported by three sessions. `open-issues.md` entry 9.

`[ -e /dev/null ]`, `[ -c /dev/null ]`, `[ -e /dev/tty ]`, `[ -c /dev/tty ]`,
`[ -e /dev/stdin ]`, `[ -p /dev/stdin ]`, `[ -w /dev/stdout ]`, `[ -e /dev/fd/0 ]` and
`[[ -e /dev/stdin ]]` are all false where Git Bash says true. The tests ask the
filesystem; each predicate needs its own answer for each name.

### 4.3 `cat <&1` reads standard input instead of failing

Reported; present before the `/dev` fixes. With `x` piped in and output a pipe,
`cat <&1` and `cat < /dev/stdout` print `x`, status 0. Bash:
`cat: -: Bad file descriptor`, status 1. Cause not looked for.

### 4.4 `cat 3< f <&3` is refused

Reported; present before the `/dev` fixes. cash:
`operation not supported on this platform: fd redirections`, status 1. Bash prints the
contents of `f`. Cause not looked for.

### 4.5 A bundled tool calls itself `cash.exe` in its messages

Reported by the user on 2026-09-30, from the installed cash (Scoop's), at the prompt:

```text
❯ cut
cash.exe: you must specify a list of bytes, characters, or fields
Try 'C:/Users/thraa/scoop/apps/cash/current/cash.exe --help' for more information.
❯ cut dfdfd
cash.exe: you must specify a list of bytes, characters, or fields
Try 'C:/Users/thraa/scoop/apps/cash/current/cash.exe --help' for more information.
```

GNU `cut` says `cut: you must specify a list of bytes, characters, or fields` and
`Try 'cut --help' for more information.` The tool runs as `cash.exe --invoke-bundled
cut`, and takes its name for messages from the program it runs in, not from the name it
was called by; `cut --help` itself says `Usage: cut OPTION... [FILE]...`, so the name is
known. To check for the other bundled tools as well: the fault is likely in the one
place they are dispatched (`crates/cash-shell/src/bundled.rs`,
`crates/cash-coreutils-builtins`), not in `cut`. It sits in this phase because 4.1 is in
the same layer.

### 4.6 Small differences left in the `/dev` names

Each is done only if it is cheap while 4.1 and 4.2 are in that code; what is not moves to
`open-issues.md`.

- `/DEV/STDIN` works in Git Bash and is a path in cash; `/dev/null/` is refused by Bash
  and is still the device in cash.
- `read x < 'CONIN$'` fails; only the `/dev/tty` name is mapped to the console.
- `exec 3<>/dev/tty` can be read from but not written to: a console has no one handle
  that is both (spec D7).

---

## Phase 5. Jobs, `wait`, and the tools beside them

### 5.1 `jobs` and `wait`: every difference from Bash 5.3 in `open-issues.md` entry 8

**Decided: all ten**, until cash and Bash 5.3 print the same for every script in that
entry's table. It has the scripts and both outputs.

- `$(jobs)` and `jobs | …` show a finished job as still running, so
  `while [ -n "$(jobs -pr)" ]; do sleep 0.1; done` never ends. A subshell gets a copy of
  the job table (`Shell::clone`, `JobManager::snapshot`) taken without polling the jobs.
  This is the one a script is likely to meet.
- **Decided:** `wait %N` after `jobs` has shown the job matches Bash,
  `wait: %1: no such job` and 127, where cash returns the job's status now
  (`collect_saved_job`); `wait $pid` keeps returning the status.
- A plain `wait` forgets more in Bash: every saved status, except that of `$!` when that
  job had ended and not been reported.
- A finished job keeps its number until it is reported, and `%N` names nothing after
  that; a cash script reaps a finished job when the next one starts, which takes its
  number. This one changes when a job gives up its number, in the job table that was
  changed twice that week.
- `wait %5` for a job that does not exist: Bash's message, and 127.
- `wait -n -p VAR` unsets `VAR` when it has nothing to return.
- POSIX mode prints a failed job as `Done(3)`.

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

---

## Decided, written down so it is not decided twice

- **`> /dev/stdout` shares the descriptor, it does not open the file again** (spec D7,
  §4 row 39). Git Bash reopens, so `{ echo a; echo b > /dev/stdout; } > out` leaves `b`
  there and `a`, `b` in cash. Both `/dev/stdin` sessions chose sharing, on their own.
- **Two sessions fixed `/dev/stdin` at the same time.** `zealous-goldstine`'s
  implementation is the one in `main` (it was built on the `/dev/tty` work). Of
  `kind-nash`'s, the tests, `open-issues.md` entry 9 and the spec row were kept and its
  implementation was not; its commits are in the history behind the merge.
