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

## Picked by the user (the current phase, 2026-10-19)

The user's picks after 1.34.0 (decisions.md, "The phase after 1.34.0"), in this order,
each described under "Found along the way" until it is done:

1. `--inherit-fd FD` does nothing: done (DONE.md);
2. a `case` of more than 64 patterns with brackets or extended globs: done (DONE.md);
3. another corpus round: JSON.sh, argbash, asdf's core and bash-oo-framework, the
   downloads approved in the same pick: done 2026-10-20 (DONE.md).

1.34.0 was tagged and not published (its cash.exe went over the size budget); 1.34.1
carried it, with the fix, to Scoop (2026-10-19).

## The phase before (2026-10-19, done)

The user's picks after 1.33.0 (decisions.md, "The phase after 1.33.0"):

1. 1.33.0 published through Scoop: tagged, released, and the bucket's Excavator run
   (2026-10-19);
2. another corpus round: bashunit, git-subrepo and shellspec done (2026-10-19,
   DONE.md); bash-commons left out (it writes through `sudo`). 1.34.0 ended it.

## The phase before that (2026-10-19, done)

The user's picks after 1.32.0 (decisions.md, "The phase after 1.32.0"), in this order,
each described under "Found along the way" until it is done:

1. the three `cd` gaps: done (2026-10-19, DONE.md);
2. `set -o history` in a script: done (2026-10-19, DONE.md);
3. ROADMAP.md section 6 brought up to date: done (2026-10-19, DONE.md);
4. the benchmark tool for all five shells: done (2026-10-19, DONE.md), the comparison
   itself the user's to run;
5. `who`, `pinky` and `users` on Windows' session API: done (2026-10-19, DONE.md);
6. `split --filter`: done (2026-10-19, DONE.md);
7. another corpus round, its sources and their terms put to the user before any
   download: done (2026-10-19, DONE.md).

## The phase before (2026-10-18, done)

The user's picks after 1.27.0 (decisions.md, "The phase after 1.27.0"), in this order.
The phases before, released as 1.22.0 to 1.27.0, are in DONE.md. Items 3 (zstd, parked)
and 4 (winget, not now) stay open.

After 1.28.0 (decisions.md, "The phase after 1.28.0"): first the four findings picked
to be fixed, as they change pages already written: the case of a Windows variable's
name, uutils' error texts, `cp`'s copy time and the alias read as text (all four done,
DONE.md); then the man batches straight on (step 7), before items 2 to 5.

1. **`man`, cash's own manual** (the user, 2026-10-18): `research/man-browser-design.md`,
   in the steps of its section 7, each ending in a commit:
   1. the pages' model, in the crate `cash-man`: done (2026-10-18, DONE.md);
   2. rendering: done (2026-10-18, DONE.md);
   3. `help` on the new pages: done (2026-10-18, DONE.md);
   4. the browser: done (2026-10-18, DONE.md), with `help`'s short view;
   5. the examples' test: done (2026-10-18, DONE.md);
   6. the first batch of about 20 pages at full depth, and drafts for the rest: done
      (2026-10-18, DONE.md); then a release;
   7. the next batches, about 20 pages each: done (2026-10-19, batches 2 to 10,
      DONE.md); every page is written in full, none is a draft. Released as 1.29.0
      (to batch 4), 1.30.0 (to batch 7) and 1.31.0 (batches 8 to 10).
2. **Document the two spellings a directory takes in `PATH`**: done (2026-10-19,
   DONE.md): `help paths`, `help differences` and spec §4 row 2.
3. **zstd's real levels**: before the switch, its side effects are looked at: the size
   of `cash.exe` (a release stops past 2 MiB more), the port's unsafe translated code,
   and the memory of levels 19 to 22.
   **zstd compresses at ruzstd's fast level only** (parked by the user, 2026-10-17;
   phase 28, 2026-10-07): every level
   `-1` to `-22` gives about zstd's `-1`, larger than zstd's default `-3`. When
   `libzstd-rs-sys` (Trifecta Tech's port of libzstd) has a Rust API, the levels can be
   real; until then the page says so. Looked at again 2026-10-19: `libzstd-rs-sys` is
   still a pre-release (`v0.0.1-prerelease.2`), its encoder the raw c2rust translation
   (Trifecta Tech seeks funding for that cleanup, "Milestone 4: Encoder
   implementation"), with no safe Rust API; parked until it has one.
4. **The winget submission** (**yours** at the time): the manifests are validated
   (ROADMAP.md section 18); the submission is a pull request to `microsoft/winget-pkgs`,
   public, which goes out only with the user's yes then. Its side effects first: each
   release then needs a winget pull request of its own, and a winget install meets the
   first-start offer that changes the user's `PATH` and default Terminal profile.
5. **More corpus rounds**: permissively licensed real scripts and cases of cash's own
   on the Bash manual (`tests/corpus/SOURCES.md`). A round done (2026-10-19, DONE.md):
   wait-for-it, shUnit2, shFlags, bashmarks and git-extras, four fixes. A next round
   needs the user's yes for its downloads.

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

- **Two oracle tests lost their directory part way, once** (2026-10-19, a full `it`
  run): `column_cases.sh` and `bzip2_cases.sh`, running at the same moment, found the
  directory `mktemp -d` had made for them gone ("Not a directory" for each program they
  started after, "No such file" for files they had just written); the run after passed.
  No test or corpus case was found removing temporary directories other than its own,
  and `mktemp -d` gave 40 cash processes started at once distinct names. Watch for it.

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

---

## Decided, written down so it is not decided twice

In [decisions.md](decisions.md), by topic (moved 2026-10-11); new decisions go there.
