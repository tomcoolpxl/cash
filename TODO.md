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
- **Releases:** what was on `main` on 2026-09-30 is 1.3.0; each phase ends with a release
  of its own. A tag is made only after CI has passed on the commit (RELEASING.md).
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before. All of
their work is in `main`; the sessions themselves are not needed any more. The order and
the grouping were chosen that day: the test suite first, so that every later phase can
trust its runs, then by what a user notices most.

| Phase | What | Items |
| --- | --- | --- |
| 0 | Release 1.3.0 | what is on `main` |
| 1 | A test suite that can be trusted | 1.1 to 1.3 |
| 2 | Ctrl-C everywhere | 2.1 to 2.3 |
| 3 | Process substitution | 3.1, 3.2 |
| 4 | The `/dev` names and descriptors | 4.1 to 4.5 |
| 5 | Jobs, `wait`, and the tools beside them | 5.1 to 5.3 |

---

## Phase 0. Release 1.3.0

`main` was pushed on 2026-09-30 (`d947c837`) with 34 commits since `v1.2.1`: `read` at
the console (`-t`, `-d`, `-n`, `-s`, Ctrl-C), `/dev/tty`, `/dev/stdin` and the other
descriptor names, standard input read without a buffer, `kill` and reused pids, a finished
job's status, `top`, `coolfetch`. Once CI is green on it: bump the version to 1.3.0, tag,
and let the Scoop bucket pick it up, as RELEASING.md says.

---

## Phase 1. A test suite that can be trusted

Decided: fix the tests whose timing fault can be named, fence the ConPTY tests, leave the
ones that cannot be reproduced on a quiet machine, with a note.

### 1.1 `stdout_capture` sometimes fails while nextest lists tests, and no test runs

Seen twice on 2026-09-30, and it passed on the rerun both times. nextest asks each test
binary for its tests with `--list`; `stdout_capture` of cash-win32 has a harness of its
own, runs its cases instead, and `the_handle_is_restored_even_if_the_body_panics` fails
at `crates/cash-win32/tests/stdout_capture.rs:233` with "the panic was swallowed". The
whole run then ends with exit code 104 before any test has started. Not looked into.

### 1.2 `cargo doc -p cash-win32` fails

Reported: four broken doc links in `crates/cash-win32/src/children.rs` and `ctrl_z.rs`.

### 1.3 Tests that fail when the machine is busy

Each failed once in some session's full run (`--retries 0`) while other sessions were
building, and passed when run alone.

- With a fault that can be named, to be made to wait for a condition:
  - `read_console::a_script_with_both_streams_redirected_asks_the_terminal_by_name`: the
    answer is typed 300 ms after the query is shown, and is swallowed by the console if
    the inner cash has not begun its read by then.
  - `pipeline_concurrency::a_while_loop_does_not_lose_output` (timed out).
- ConPTY tests, to get a nextest group that limits how many run at once:
  - `pty-oracle` `cash_leaves_the_screen_bash_leaves` (different cases each time; it
    already runs alone).
  - `conpty_ctrl_z_stops_a_foreground_program_and_fg_resumes_it`,
    `conpty_ctrl_z_stays_end_of_input_for_a_program_reading_the_keyboard`,
    `conpty_interactive_variables_and_arithmetic`.
- To look at once, and leave with a note if they do not fail on a quiet machine:
  - `bash_gaps::wait_preserves_status_and_waits_for_next_completion`,
    `bash_gaps::a_chld_trap_runs_once_for_each_child_reaped`
  - `job_groups::at_the_prompt_a_background_job_survives_ctrl_c`
  - `git_prompt::git_ps1_shows_the_branch_its_state_and_the_upstream` (timed out)
  - four `cash-sed` tests, `completion_scripts::docker_completion_works`
  - `fuser_lsof::fuser_marks_an_executable_and_a_loaded_module`: given three periods of
    30 s in `.config/nextest.toml`; item 5.3 is its cause.

---

## Phase 2. Ctrl-C everywhere

### 2.1 Ctrl-C does not end a script outside `read`

Checked on a ConPTY, `cash script.sh`, Ctrl-C handling enabled in the parent.

```bash
trap 'echo bye' EXIT
ping.exe -n 4 127.0.0.1 > /dev/null
echo "after-ping rc=$?"
i=0; while [ $i -lt 300000 ]; do i=$((i+1)); done
```

- During `ping.exe`: ping dies, and the script goes on with `after-ping rc=137`.
  `ChildProcess::wait_or_stop` (`crates/cash-core/src/processes.rs`) and
  `Job::wait_in_foreground` (`jobs.rs`) wait on `await_ctrl_c()` and do nothing with it.
- During the loop of builtins: nothing listens, Windows ends cash with `0xC000013A`, and
  neither the `EXIT` trap nor a trap on `INT` runs.
- Git Bash 5.3: the script ends at once in both cases, `bye` is printed, status is
  SIGINT's; with a trap on `INT` the trap runs and the script goes on.

**Decided: the script ends only if the program died of the Ctrl-C**, as in Bash: it ended
with Windows' `0xC000013A`, or cash's relay killed it. Then the script ends with 130 and
its `EXIT` trap runs, or its trap on `INT` runs instead. A program that handled Ctrl-C
itself and lived on or exited normally (a REPL, `python`, `terraform apply` finishing its
step) leaves the script running. D13 stays as it is.

To build on: `ErrorKind::Interrupted` ends a script with 130 and abandons the line at the
prompt (`Error::to_control_flow`, `Program::execute`), and `run_interrupt_trap` in
`read.rs` runs a trap on `INT`.

### 2.2 Ctrl-C in `select` and `mapfile` at the console

Checked. Both still read the console through the standard library, as `read` did.

```bash
trap 'echo bye' EXIT
select x in one two; do echo "chose $x"; break; done; echo "after-select rc=$?"
```

- Ctrl-C enabled: Windows ends cash (`0xC000013A`), no `EXIT` trap.
- Ctrl-C ignored (a test runner's children): nothing happens; after Enter the script goes
  on. `mapfile -t lines` behaves the same way.
- Git Bash: the script ends at once and `bye` is printed.

`select` reads its answer in `crates/cash-core/src/interp.rs`; `mapfile` looks for 0x03
in bytes a console collecting lines never hands over. `ConsoleInput` in `read.rs` does the
job for `read` but lives in cash-builtins, and `select` is in cash-core.

### 2.3 Small differences left in `read` at a console

Each is done only if it is cheap once 2.2 has moved the console's key reading; what is
not moves to `open-issues.md` as a known difference.

- Tab in a line is shown as `^I`, where a terminal shows blanks up to the tab stop
  (erasing it would need the column it began in).
- Keys typed past the count of `-n` are shown when something reads them, not when typed.
- A line not ended when `-t` runs out is dropped; a terminal keeps it for the next read.
- An answer to a terminal query that arrives before `read` has begun can be swallowed by
  an older console host (Windows Terminal passes it through).
- The console's own line editing (arrow keys, Escape, function keys) is gone from a plain
  `read`, the price of Ctrl-C working there; `read -e` has an editor.

---

## Phase 3. Process substitution

### 3.1 A running `>(...)` is lost when cash exits, and can leave a process behind

Reported by the session that stabilised `write_process_substitution_works`.

- `cash -c 'echo x > >(cat)'` with stdout a file printed nothing in 9 of 12 runs;
  `cash -c 'echo x > >(sleep 1; cat)'` never prints `x`. The substitution is a thread of
  the shell and ends with it; `wait` does not cover it (`$!` is empty). In Bash it is a
  process that outlives the shell, so the output still arrives.
- About 1 run in 15 left a `cash.exe --invoke-bundled cat` whose only thread was
  suspended. It never ends and locks `target\debug\cash.exe` ("Access is denied" on the
  next build). Likely the shell exits between creating the child suspended and resuming
  it; not proven.

**Decided: cash waits for its running substitutions before it exits**, so their output
always arrives, as it does in Bash. One that never ends keeps cash from exiting.
`wait $!` for a substitution is not asked for. The leftover suspended process is fixed
with it, and `spec.md` D17, which still describes the old temp-file model, is rewritten.

### 3.2 `tee >(cmd)` fails

Reported; present before the `/dev` fixes. `open-issues.md` entry 9.

```bash
echo x | tee >(cat >&2)
```

cash: `\\.\pipe\cash-procsub-…: The parameter is incorrect.`, status 1. Bash: `x` on both
streams. Cause not looked for.

---

## Phase 4. The `/dev` names and descriptors

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
argument. A program on `PATH` still gets the name as written and fails. This is a new
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

### 4.5 Small differences left in the `/dev` names

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
