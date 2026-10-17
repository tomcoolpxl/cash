# To do

Everything found while working on cash that is not done yet, in one list, and the plan
for working through it. What is done, phases and findings, is in DONE.md.

## How this is worked
- **One session, one item at a time, in the order below.** No further worktrees and no
  spawned sessions: what is found on the way is added to the phase it belongs to, or to
  the end, not started beside it.
- **The user picks the items** (2026-10-17): before items are started, they go to the
  user as a pick list, each with what it fixes, the danger of leaving it and the size of
  the work. The phase is what the user picked.
- **Per item:** reproduce it (the code has moved since the item was written down),
  compare with Git Bash 5.3, fix it with a test that fails without the fix, and commit.
  A done item moves to DONE.md in the same commit, as it was written down.
- **Per phase:** `cargo fmt --all`, `cargo clippy --workspace --all-targets`, the full
  nextest run (`--profile full --retries 0`, both lanes); then `main` is brought up to the branch and pushed,
  the phase is released once CI is green, and a short report is written. The next phase
  starts once the user has picked its items. The work stops only for a decision marked **yours** below, or
  for a release whose CI fails.
- **Releases:** each phase ends with a release of its own. A tag is made only after CI
  has passed on the commit (RELEASING.md); the releases so far are in DONE.md.
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

---

## Picked by the user (the current phase, 2026-10-17)

The user's picks after 1.23.0 (decisions.md, "The phase after 1.23.0"), in this
order. The phases before, released as 1.22.0 and 1.23.0, are in DONE.md.

1. **A TERM trap never runs on cash's own `kill -TERM`** (2026-10-17, found with
   `timeout`): `kill -TERM` sends Ctrl-Break to the job's group (D21), and cash's console
   handler takes Ctrl-C only, so a cash script with `trap … TERM` ends at once with 143,
   its trap not run, its foreground child ended with it. Bash runs the trap once the
   foreground command ends and goes on; without a trap it ends with 143; an interactive
   bash ignores TERM. cash could keep a Ctrl-Break as a pending TERM where a trap is set
   (or always, ending with 143 at the next safe point), as it keeps a Ctrl-C (D13).
   Seen: `cash -c "trap 'echo t' TERM; sleep 3; echo after" & sleep 1; kill -TERM $!`
   ends 143 with nothing printed; bash prints `t` and `after`.
2. **An archiver killed at once hung past 15 s in CI** (2026-10-17, run 39043122930,
   slow lane 4/4): `background_builtins::an_archiver_stopped_in_its_walk_writes_no_archive`
   runs `zip`, `rar` and `7z` on 3000 files as `tool & kill %1; wait %1` and was ended at
   15 s once, then passed in 1.3 s. The whole archive takes about a second, so it
   waited, not worked: plausibly a `kill` that lands before the builtin reads its stop,
   with `wait` left waiting. Which of the three it was is not known; the test should name
   the tool it is on, and a loop of the three under load should catch it. On run
   39066508176 (2026-10-17) it and two other tests that `kill` background jobs
   (`busy_background_jobs_leave_the_foreground_free`, 40 busy loops, and
   `a_compressor_kill_stops_on_standard_input_leaves_a_broken_archive`) timed out at
   15 s on all four tries, side by side, on a runner whose tool install had failed; the
   next run, with the same code, passed all three. A starved runner, then, or the same
   wait, which load brings out.
3. **The completion menu scans PATH on each keystroke**: the completion menu runs the PATH scan (or carapace) again on every keystroke while
   it is open: each folder is asked for the names beginning with the prefix since
   2026-10-17, 17.7 ms for `compgen -c no` where listing each whole took 24. The rest
   is a directory query for each of the 80 folders on PATH, at Windows' cost of one,
   and the runnable check of each match; the background `PathIndex` keeps folded
   names only, and would need the names as spelled and each one's runnable answer.
4. **`which <builtin>` walks all of PATH**: `which <builtin>` and `type -a` scan all of PATH on every call: `type -a` must; for
   a bundled tool `which` looks along all of PATH for a `cash --link-tools` link to
   this cash, comparing each match with it (looked into 2026-10-17; `which` is rarely
   in a loop).
5. **Each `.ps1` run scans PATH for pwsh**: each `.ps1` run scans PATH for pwsh (looked into 2026-10-17): one walk for `pwsh`,
   then `powershell`, next to PowerShell's own start of a few hundred milliseconds.
6. **`HISTCONTROL=erasedups` scans the whole history**: `HISTCONTROL=erasedups` scans the whole history (50,000 entries) after each command
   (looked into 2026-10-17): 1 to 2 ms after Enter, which the starter `.bashrc` turns
   on; an index from command line to entries would make it a lookup, at the cost of
   keeping that index right through loading, `history -d` and trimming to `HISTSIZE`.

## Left over from finished phases

What the phases now in DONE.md left open, each where its phase says more. Not now
(the user, 2026-10-17): each needs unusual input to matter.

- **7z: a tar name with a lone surrogate** (2026-10-12): a UTF-8-encoded surrogate with
  no partner (`ED A0 80`) stays a lone UTF-16 unit in 7-Zip's listing and extracted
  name; cash's names are Rust strings, so it shows and writes U+FFFD. Pairs (CESU-8)
  are joined, as 7-Zip joins them.
- **Updating a solid encrypted RAR 5 archive without its password** (phase 33): a
  non-solid archive's members are carried as they are, but a solid one's are read back
  and written again, which needs the password.
- **RAR dictionaries above 4 GB** (RAR 7's, phase 33) are not tried.
- **`sudo -E`'s variables through a handle** (phase 35,
  `research/sudo-in-terminal-design.md` section 3 item 4): they still go as `NAME=value`
  words on the started cash's command line (Windows' 32,000 characters for the elevated
  side, 1,024 for the as-user side); an anonymous pipe the started cash takes, as it takes
  the standard handles, would lift the limit.

## Found along the way

- **A benchmark of 1.10.0's compressors** (parked by the user, 2026-10-17, decisions.md;
  the user asked, 2026-10-07; best of two runs,
  one tool at a time, a 19 MB tar of the sources and the 51 MB `cash.exe`, against
  Scoop's programs and WSL2's). Ahead: gzip (0.09 s against 0.4–0.5 s, 0.3 s against
  2 s), bzip2 (0.16 s against 1 s, 0.5 s against 3.3 s), zip and gzip decompressing.
  Behind, worth work: **7z compresses at half 7-Zip's speed** at the same size (again
  2026-10-11: 10.1 s against 4.4 s on a 31 MB tar, 20.5 s against 9.3 s on a 49 MB
  `cash.exe`): 7-Zip's LZMA2 runs its match finder on a thread of its own, which
  lzma-rust2 cannot, so cash is on one thread for each block of four dictionaries.
  Smaller blocks (`c=`) are as fast as 7-Zip or faster, but 2–4% larger, and the user
  wants the smaller archives (DONE.md); a match-finder thread (7-Zip's `MatchFinderMt`)
  in lzma-rust2 would give both; **zstd writes files 1.5×
  zstd's size, at half its speed** (ruzstd's fastest level, 5.9 MB against 4.0 MB);
  **xz is 20% slower on one block** (6.5 s against 5.4 s; lzma-rust2's encoder) and
  decodes at two thirds of XZ Utils' speed; zip on one large member has no parallelism
  to use (2.2 s against 1.9 s).

- **Work done again on every call, from the 2026-10-16 survey** (after `ls` was found
  re-reading Windows Terminal's settings per listing; the session facts now have one
  place, `cash_win32::facts`). Still worked out each time:
- **A Ctrl-C or `kill` on a large `xz` waits for its blocks** (parked by the user,
  2026-10-17, decisions.md; found 2026-10-16): lzma-rust2's
  threaded encoder takes the whole input at once (13 blocks of 24 MiB from a 300 MB
  file, on 16 threads) and writes the first block only when it is done, about 12 s
  later; a block cannot be stopped half way, so the stop is acted on at that first
  write (`StopToken::writer`). Nothing is lost: the archive is abandoned and the file
  kept. Acting at once would need the coding on a thread of its own, left to wind down
  writing nowhere, or a stop inside lzma-rust2's block loop. Looked into 2026-10-17:
  in lzma-rust2 0.22.0 each worker (`xz/writer_mt.rs`, `worker_thread_logic`) codes a
  whole block in one `write_all`, and `WorkPool::shutdown` joins the workers, so a drop
  waits out every block in flight. Writing the block in pieces and asking
  `shutdown_flag` between them would end it within a piece; that means taking
  lzma-rust2 (20.5k lines) in as cash's own, as crossterm was (`[patch.crates-io]`).

- **zstd compresses at ruzstd's fast level only** (parked by the user, 2026-10-17;
  phase 28, 2026-10-07): every level
  `-1` to `-22` gives about zstd's `-1`, larger than zstd's default `-3`. When
  `libzstd-rs-sys` (Trifecta Tech's port of libzstd) has a Rust API, the levels can be
  real; until then the page says so.

---

## Decided, written down so it is not decided twice

In [decisions.md](decisions.md), by topic (moved 2026-10-11); new decisions go there.
