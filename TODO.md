# To do

Everything found while working on cash that is not done yet, in one list, and the plan
for working through it. What is done, phases and findings, is in DONE.md.

## How this is worked

- **One session, one item at a time, in the order below.** No further worktrees and no
  spawned sessions: what is found on the way is added to the phase it belongs to, or to
  the end, not started beside it.
- **Per item:** reproduce it (the code has moved since the item was written down),
  compare with Git Bash 5.3, fix it with a test that fails without the fix, and commit.
  A done item moves to DONE.md in the same commit, as it was written down.
- **Per phase:** `cargo fmt --all`, `cargo clippy --workspace --all-targets`, the full
  nextest run (`--profile full --retries 0`, both lanes); then `main` is brought up to the branch and pushed,
  the phase is released once CI is green, and a short report is written. The next phase
  starts without waiting. The work stops only for a decision marked **yours** below, or
  for a release whose CI fails.
- **Releases:** each phase ends with a release of its own. A tag is made only after CI
  has passed on the commit (RELEASING.md); the releases so far are in DONE.md.
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

---

## Left over from finished phases

What the phases now in DONE.md left open, each where its phase says more.

- **7z cases not yet checked against 7-Zip** (phase 31): tar names that are not ASCII
  (7-Zip prints its UTF-8 check's status after "UTF8"); zips 7-Zip reads by their local
  headers (no central directory, "Local" in Characteristics); split zips; Unix-made
  zips' names and modes.
- **Updating a solid encrypted RAR 5 archive without its password** (phase 33): a
  non-solid archive's members are carried as they are, but a solid one's are read back
  and written again, which needs the password.
- **RAR dictionaries above 4 GB** (RAR 7's, phase 33) are not tried.
- **Helix mode in `cash-reedline`** (phase 34): on by default in reedline, so compiled
  into cash, though cash never offers it; its code runs through the core editor, so
  taking it out is a change of its own.
- **The 2024 edition for `cash-reedline` and crossterm** (phase 34): both keep the 2021
  edition they came with.
- **`sudo -u USER` with a windowed program, not visually checked** (phase 35): cash now
  grants USER the window station and desktop while the command runs (`winstation.rs`),
  which should let a window show, and puts the grant back after; a GUI run as another
  account has not been watched on screen. Try `sudo -u USER notepad` and see it open.
- **`sudo -E`'s variables through a handle** (phase 35,
  `research/sudo-in-terminal-design.md` section 3 item 4): they still go as `NAME=value`
  words on the started cash's command line (Windows' 32,000 characters for the elevated
  side, 1,024 for the as-user side); an anonymous pipe the started cash takes, as it takes
  the standard handles, would lift the limit.
- **UAC's prompt for `sudo` often only blinks in the taskbar** (phase 35, 2026-10-10):
  when the asking process is not in front, Windows shows the consent dialog as a
  flashing shield, and the user missed three prompts that way; left long enough, a
  prompt counts as declined. gsudo (MIT) brings it forward: for a second after asking,
  `FindWindow("Credential Dialog Xaml Host")` every 100 ms, then `SetForegroundWindow`
  (`src/gsudo/Helpers/UACWindowFocusHelper.cs`). Do the same in
  `cash_win32::elevate::run_elevated_here`.
- **A bundled tool outlived its script and its pseudo console** (2026-10-10): after a
  `sudo_in_terminal` probe timed out and dropped its pseudo console, the script's last
  command, `cash.exe --invoke-bundled cat got.txt` (PID 26528, its parent gone, no
  path readable), was still there and held `target\debug\cash.exe`, so the next build
  could not replace it. Find what it waits on (a write to the closed console?) and why
  the script's job did not end it.

## Found along the way

- **The ConPTY tests that run PowerShell time out in CI now and then**: PowerShell
  started by the test prints nothing in 30 s (`conpty_interactive_tests.rs:220`).
  Run 37953062174 (2026-10-09, slow lane 1/4, both tries):
  `conpty_a_program_that_ran_leaves_a_healthy_prompt_where_it_was`, on a runner whose
  setup also logged a "bash startup failure". Run 37963172593 (the same day, slow lane
  4/4, both tries): `..._leaves_a_scroll_region_...`, `..._dies_in_full_screen_...` and
  `..._had_vt_input_on_does_not_eat_keys_typed_ahead` (line 402). Runs between passed;
  the commits touched only rar. Look at PowerShell's start on the runner (a first-run
  cache?) before raising the wait.
- **7z's times and 7-Zip's differ by an hour across daylight saving** (found on
  2026-10-07 with a RAR's November creation time): 7-Zip turns every UTC time into local
  time with `FileTimeToLocalFileTime`, the offset of the day it runs, where cash's 7z
  uses the zone's rules for the time's own date. Written down in the page's notes; the
  oracles run cash in UTC and take 7-Zip's times back by the day's offset. Whether 7z
  should follow 7-Zip here is **yours**.
- **A benchmark of 1.10.0's compressors** (the user asked, 2026-10-07; best of two runs,
  one tool at a time, a 19 MB tar of the sources and the 51 MB `cash.exe`, against
  Scoop's programs and WSL2's). Ahead: gzip (0.09 s against 0.4–0.5 s, 0.3 s against
  2 s), bzip2 (0.16 s against 1 s, 0.5 s against 3.3 s), zip and gzip decompressing.
  Behind, worth work: **7z compresses at half 7-Zip's speed** (5.6 s against 2.9 s,
  17.7 s against 8.9 s, the same sizes): 7-Zip's LZMA2 runs its match finder on a
  thread of its own and splits large input across threads; **zstd writes files 1.5×
  zstd's size, at half its speed** (ruzstd's fastest level, 5.9 MB against 4.0 MB);
  **xz is 20% slower on one block** (6.5 s against 5.4 s; lzma-rust2's encoder) and
  decodes at two thirds of XZ Utils' speed; zip on one large member has no parallelism
  to use (2.2 s against 1.9 s).
- **Three zip choices made without a pick list** (phase 30, 2026-10-07, under "do not
  stop"): deflate on `miniz_oxide`, the RID as the numeric owner, and "made on Unix" (design
  3.13). **Yours** to confirm or change.
- **`TAR_OPTIONS` and the command line** (phase 29, 2026-10-07): GNU tar drops the
  `TAR_OPTIONS` side of `--one-top-level` against `-P` given on the command line; cash
  refuses the pair wherever they come from.
- **zip -v's space before the tab** (phase 30, 2026-10-07): zip prints it where its
  progress dots would start; cash prints it for a stored member and for one of 64 KiB
  or more, which matched every case tried. A file between those may differ.
- **zstd compresses at ruzstd's fast level only** (phase 28, 2026-10-07): every level
  `-1` to `-22` gives about zstd's `-1`, larger than zstd's default `-3`. When
  `libzstd-rs-sys` (Trifecta Tech's port of libzstd) has a Rust API, the levels can be
  real; until then the page says so.
- A test in the full suite leaves `x.lnk` in the repository root: a 0-byte named pipe as
  MSYS2 makes them (its `mkfifo` writes a FIFO as a special `.lnk` file), timestamped
  during the 1.4.3 gate on 2026-10-06. No test names `x` with `mkfifo`; one of them runs
  with the repository as its folder. Find it and give it a temporary folder.
- **`cp src >(cat)` can print zeros** (found 2026-10-10 in the 1.12.0 gate, once in a
  full run): a `>(...)` handed to a tool not patched for pipes is a temp file read as it
  grows, and `cp` (through `CopyFileEx`) sizes its target before it writes it, so a read
  in between passes on NULs. The test
  (`acceptance::an_unpatched_bundled_tool_writes_into_a_write_substitution_through_a_file`)
  is `#[ignore]`d. A fix reads only what was written: hold back the tail of a file that
  grew in one step until the program is done, or give `cp` the pipe as tee's patch did.
---

## Decided, written down so it is not decided twice

- **What cash prints never cites its own documents** (the user, 2026-10-05): no
  decision numbers, §4 rows, `spec.md`, ROADMAP items or research files, in help pages,
  `--help`, version lines, warnings or `cash doctor`. `ss --help` ended with "cash's ss
  is the Windows subset described in ROADMAP item 9"; help pages had 122 such
  references. Point to a `help` page instead.

- **An elevated `ss -i` switches statistics collection on** (the user, 2026-10-05):
  Windows keeps per-connection TCP statistics (rtt, rto, cwnd, bytes, retransmits,
  queues) only for connections an administrator switched collection on for; unelevated,
  only the MSS is readable and the rest reads as zeros or garbage. So unelevated `-i`
  prints the MSS with a note pointing to `sudo ss -i`, and elevated `-i` switches
  collection on for the listed connections (on until each closes) and prints the rest,
  counted from that moment.
- **Files an elevated `sudo` makes are yours, and an approver who is not you is named**
  (the user, 2026-10-04): when you approve with your own account, the elevated side sets
  your account as the default owner of what it creates (Unix leaves root owning them, and
  Windows `Administrators`); when another account approves (a standard user), cash says
  `running as ADMIN, not you`. Leaving Windows' default, and a system-wide policy, were
  turned down.

- **`elevate` stays, beside `sudo` and `su`** (the user, 2026-10-04): it starts a
  program elevated in a new window without waiting and needs no tool; `sudo` and `su`
  fall back to its UAC request when neither gsudo nor Windows' sudo is there. Removing
  it, or keeping `sudo` refusing without a tool, was turned down.

- **Bundled tools stay processes of their own** (the user, 2026-10-04): a call to
  `cut`, `sort`, `sed` or `awk` costs a process start, about 25 to 35 ms in a debug
  build against 0.5 ms for pure shell, and a tool that crashes or exits cannot take the
  shell with it. Running them inside the shell's process was turned down. A bundled
  `grep` likewise: Git for Windows' GNU grep is always there (D35), costs the same per
  call, and handles CRLF.

- **Uptime is Windows' figure** (the user, 2026-10-04): `coolfetch`, `uptime` and `top`
  count from the last full kernel boot (`GetTickCount64`), as Task Manager and fastfetch
  do, so with Fast Startup a power-on resumes the count rather than restarting it.
  Counting from the last power-on (the Kernel-Boot event), or showing both, was turned
  down.

- **sed's errors read as GNU sed's, prefix and all** (the user, 2026-10-04):
  `sed: -e expression #1, char 5: unknown command: `x'`. Keeping cash's
  `<script argument 1>:1:5: error:` before GNU's words was turned down.
- **1.3.14 goes out with phase 14's list done; the GNU gaps found on the way are phase
  15** (the user, 2026-10-04). Another round before the release was turned down: each
  round of comparing had found more.

- **awk's `system()` and pipes and sed's `e` run plain cash** (the user, 2026-10-04):
  `cash -c`, `$0` `cash`, Bash's behaviour. Running them as `sh`, in POSIX mode, as
  gawk and GNU sed run `/bin/sh`, was turned down.

- **`exec -a NAME` is refused for a program other than cash** (the user, 2026-10-04): std's `Command` cannot
  set a program's `argv[0]`, and running it under its own name in silence was the
  alternative to an error. A spawn of cash's own to set it, and leaving it ignored, were
  turned down. `exec -l`, which only puts `-` before `argv[0]`, goes with it.
- **Times honour `TZ`, as in Bash** (the user, 2026-10-04): `printf '%(…)T'`, prompts and
  `HISTTIMEFORMAT` format in the zone `TZ` names (`UTC`, a POSIX `EST5EDT`, an IANA name),
  else Windows' own. Noting that cash ignores `TZ` was turned down.
- **W32-02 gets one timed investigation** (the user, 2026-10-04): a reproduction rate,
  then the server waiting for its reader before it closes, kept only if the rate drops;
  else it is recorded as a `cmd` quirk and closed.

- **A cash started as `sh` runs in POSIX mode, as Bash does** (the user, 2026-10-04):
  `sh -c`, `exec sh`, `#!/bin/sh` and `#!/usr/bin/env sh` turn on `set -o posix`, every
  builtin kept. Keeping D7's "no modes", with `sh` plain cash, was turned down. Spec D7.
- **A program in a folder too long for Windows starts in its 8.3 short name** (the user,
  2026-10-04), and fails saying why where there is none that fits. A clear error alone,
  as Git Bash gives for native programs, was turned down. Spec D29, §4 row 45.

- **The MSRV is the pinned toolchain's minor, 1.98** (the user, 2026-10-04): it is the
  only Rust CI builds with, so it is the only one promised; it rises with
  `rust-toolchain.toml`. A 1.95 floor with a CI job of its own, and no `rust-version`
  at all, were turned down.

- **The git-prompt goldens are frozen against the installed Git for Windows** (the user,
  2026-10-04): `tests/git-prompt/SOURCES` holds the hashes of the scripts the output came
  from, and the test fails and says to re-freeze when they change, a runner's Git update
  included; without Git for Windows it fails. Vendoring the GPL-2.0 `git-prompt.sh`, and
  keeping the differential a by-hand script, were turned down.

- **History is not cut while `HISTSIZE` and `HISTFILESIZE` are unset** (the user,
  2026-10-03): every line is kept, where Bash defaults both to 500. Spec §4 row 43.
  Following Bash, which would cut an existing long `~/.cash_history`, was turned down.
- **`\v` and `\V` in a prompt give cash's version** (the user, 2026-10-03), as `\s`
  gives `cash`. Spec §4 row 44. Following `$BASH_VERSION` was turned down.
- **Errors are worded as Bash words them** (the user, 2026-10-03): `script.sh: line 3:
  foo: command not found` in a script, `cash: line 1: …` for `-c`, `cash: foo: command
  not found` interactively, with Bash's message text (`r: readonly variable`,
  `x: unbound variable`), builtins' messages included. Keeping cash's words with Bash's
  location, and `error:` with a location added, were turned down.
- **`fg` and `bg` need job control, as in Bash** (the user, 2026-10-03): in a script
  they say `fg: no job control` and return 1, until `set -m`. Keeping cash's leniency,
  where `fg` in a script waited for the job, was turned down.
- **Errors are coloured only on a terminal** (the user, 2026-10-03): the prefix is red
  when stderr is a terminal and `NO_COLOR` is unset; a pipe or a file gets plain text,
  through one helper for every stream. No colour at all was turned down.
- **A builtin's bad option is Bash's two lines** (the user, 2026-10-03): `name: -q:
  invalid option` and `name: usage: <synopsis>`, status 2. Keeping clap's block was
  turned down. `name --help` goes to stdout with status 2, as Bash's does (the user,
  2026-10-03, correcting the status 0 first written here); `help name` gives 0.

- **`chmod` is silent about group and other bits** (the user, 2026-10-03): Windows has
  none per file, so `chmod go-w ~/.ssh` and `chmod o+r f` change nothing and say nothing,
  as a numeric mode's other bits do. Spec §4 row 18. Warning on grants only, or on all of
  them, were turned down.

- **Tab globs a typed `*` and `?`, and completes everything else as typed** (the user,
  2026-10-03): no Windows name can hold `*` or `?`, so `**/nee` keeps completing, and
  `[draft]` now does. Spec §4 row 40, LANG-16. Completing all of it literally, as Bash
  does, and leaving every glob character a glob, were turned down.
- **The D31 case-insensitive fallback keeps reaching every variable** (the user,
  2026-10-03): an unset `$seconds` still reads `SECONDS`; only its speed and the order
  among `Foo`/`FOO` changed (LANG-15). Limiting it to exported or imported names was
  turned down.

- **A writer whose reader went away ends with 141 in silence, if it is cash's own** (the
  user, 2026-10-03): builtins and bundled tools, as SIGPIPE ends Bash's; a program on
  `PATH` ends as it chooses. D71.

- **`$!` for a background job that starts no program is a number of cash's own** (the
  user, 2026-10-03): 4n + 1, which no Windows process has; `kill`, `wait` and `jobs -p`
  take it to mean the job. And **at most 256 subshells at once**, as Git Bash's
  `ulimit -u` says, documented with `CASH_MAX_SUBSHELLS`. Both are D70.

- **The code review's findings are worked as phases 8 to 14** (the user, 2026-10-02):
  crashes and security first, with CI hardening pulled into phase 8 and the `it` test
  helper into the start of phase 9.
- **`chmod +x` is silent and returns 0, as D23 says** (the user, 2026-10-02). The
  execute bits of a numeric mode are silent too; `chmod -x` keeps its D34 warning.
  Keeping the warning and adding the execute ACE were turned down.
- **D36 is measured before it is built or dropped** (the user, 2026-10-02): the pool is
  built only if it saves a noticeable amount (about 5 ms a prompt); otherwise D36 and
  D6's table are amended, with the measurement. Measured 2026-10-03: the job setup is
  0.18 ms a spawn against 128 ms for a Starship prompt (`prompt_job_cost` example), so
  the pool was dropped and D36 and D6's table amended.
- **`time` and `times` report real CPU time** (the user, 2026-10-02). Documenting zeros
  as a divergence was turned down.
- **The first Tab inserts the shared part, the second opens the grid on the hinted
  candidate** (the user, 2026-10-02; spec D40, reedline patches 6 and 7). One Tab used
  to do both, against what D40 said. Bash, which beeps on the second Tab and lists on
  the third, and keeping one Tab were turned down.
- **`scoop update cash` from within cash: the user's Scoop ignores running processes**
  (the user, 2026-10-02, on their machine; no code change). Tried before deciding:
  with `ignore_running_processes` set, the update goes through while cash runs. The
  running ones keep their version, and `current`, `cash.shim`, the tool links (a
  running one too) and the Terminal fragment move to the new one. Only
  `shims\cash.exe` stays as it was while a cash started through the shim runs. Scoop
  prints two errors about it, but the file is Scoop's generic shim and keeps working.
  Scoop has no switch per manifest or per command for this check. Other users get the
  manifest's note. A `cash --update`, and an update run after the last cash exits,
  were turned down.
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
- **Archive tools: what is left out on purpose** (phases 29 and 30, 2026-10-07; zip's
  repair, splits and line ends, unzip's other methods, AES and wildcard archives added
  the same day). zip refuses `-R`, `-U`, `-A`, `-J`, `-DF`, `-sp`, `-AC`/`-AS` and the
  logs; unzip skips tokenized, PKWARE DCL, Terse, LZ77 and WavPack members. zip's `-sv`
  names the parts after writing, not as Info-ZIP does while it writes. Each can be added
  when someone needs it.
