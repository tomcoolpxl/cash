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

The user's picks after 1.23.0 (decisions.md, "The phase after 1.23.0"), the first two
added after the benchmark at 1.23.0 found `ls -la` 20% slower, in this
order. The phases before, released as 1.22.0 and 1.23.0, are in DONE.md.

1. **`ls -la` 12% slower for keeping the access time**: done (2026-10-18, DONE.md).
2. **`ls -l`'s file facts from the folder listing**: done (2026-10-18, DONE.md).
3. **A TERM trap never runs on cash's own `kill -TERM`**: settled as a limitation, not
   fixed (the user, 2026-10-18; decisions.md, DONE.md).
4. **An archiver killed at once hung past 15 s in CI**: settled, load on CI's runners
   (2026-10-18, DONE.md).
5. **The completion menu scans PATH on each keystroke**: done (2026-10-18, DONE.md).
6. **`which <builtin>` walks all of PATH**: done (2026-10-18, DONE.md).
7. **Each `.ps1` run scans PATH for pwsh**: done (2026-10-18, DONE.md).
8. **`HISTCONTROL=erasedups` scans the whole history**: done (2026-10-18, DONE.md).

All eight are done or settled; the phase ends with release 1.24.0.

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

- **Command completion offers DLLs** (2026-10-18, found with the completion menu's
  index): `compgen -c no` lists `noise.dll` from System32, as 1.23.0's scan did too: a
  file without a `PATHEXT` extension counts as runnable by its contents, and a DLL is a
  PE image (D46). Running `noise.dll` fails; the content check could take a PE image only
  when it is an executable, not a DLL (the `IMAGE_FILE_DLL` flag in its file header).
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
