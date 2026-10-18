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

## Picked by the user (the current phase, 2026-10-18)

The user's picks after 1.24.0 (decisions.md, "The phase after 1.24.0" and "The phase
after 1.25.0"), in this order. The phases before, released as 1.22.0 to 1.24.0, are in
DONE.md.

1. **Command completion offered DLLs**: done (2026-10-18, DONE.md).
2. **Busy background jobs starved the foreground**: done (2026-10-18, DONE.md). Picked
   for the phase after 1.25.0 and taken into 1.25.0, as 1.24.0's release gate timed out
   on it four times.

Released as 1.25.0. Then, for 1.26.0:

3. **Grow the differential corpus** (`tests/corpus`, ROADMAP.md section 6): new
   real-world scripts run under cash and under Git Bash 5.3 with GNU sed and gawk; each
   mismatch fixed with a focused regression, or classified in its file's `# tags:` line.

   The first round (2026-10-18): 23 new pure-bash-bible cases (MIT), the sections not
   yet in the corpus, with `tests/corpus/SOURCES.md` for every source's terms. Done
   (DONE.md): quoted alias names, aliases in aliases and after a blank, a subshell's own
   traps, and `trap -p`'s listing. The `$OSTYPE` case is a known difference (spec §4
   row 25). The second round: 10 pure-sh-bible cases (MIT), all as Bash has them, and
   semver-tool 3.4.0 (Apache-2.0) run whole, which found `.` taking `--help` and `--`
   after the script as its own (done, DONE.md). The third: JSON.sh (MIT), spark (MIT)
   and getoptions (CC0) run whole, all three as Bash has them. The fourth: seven cases
   of cash's own, the Bash manual as their spec (`${var@…}`, `declare -p`, `getopts`,
   `case` fall-through, `mapfile` and `read`, here-documents, glob options), which found
   `GLOBIGNORE` unread and a last `**` short of its folder (done, DONE.md). Left open:

   - **An alias whose value holds an operator runs it as words**: `alias multi='echo
     m1; echo m2'`, then `multi`, prints `m1 ; echo m2`; Bash runs both commands.
     Aliases such as `alias gs='git status && git diff'` are common in a `.bashrc`.
     cash expands an alias after the command's words are parsed (a TODO taken over
     from brush, #57), so only words can come of it; Bash expands it as the line is
     read, before parsing. The fix moves alias expansion into the parser. Large.
   - **extglob is on from the start** (`shopt extglob` says `on`; `+(x)` matches before
     `shopt -s extglob`), where Bash has it off: cash parses a whole script with one
     set of options, so a script that turns extglob on later would not parse otherwise
     (`shell.rs`, a TODO taken over from brush). Bash parses a command at a time. Only
     scripts that rely on extglob being off see it.
   - **`GLOBIGNORE` does not show in `shopt dotglob`**: a glob finds dot files while it is
     set, as in Bash, but Bash also turns the dotglob option itself on when `GLOBIGNORE`
     is set and off when it is unset, so `shopt dotglob` says `on`, and a dotglob turned
     on by hand ends with the unset. cash has no hook from a variable to an option. Small.

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
