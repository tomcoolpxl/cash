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

The user's picks after 1.27.0 (decisions.md, "The phase after 1.27.0"), in this order.
The phases before, released as 1.22.0 to 1.27.0, are in DONE.md.

After 1.28.0 (decisions.md, "The phase after 1.28.0"): first the four findings picked
to be fixed, as they change pages already written: the case of a Windows variable's
name, uutils' error texts and `cp`'s copy time (all three done, DONE.md), and the alias
read as text (under "Found along the way"); then the man batches straight on (step 7),
before items 2 to 5.

1. **`man`, cash's own manual** (the user, 2026-10-18): `research/man-browser-design.md`,
   in the steps of its section 7, each ending in a commit:
   1. the pages' model, in the crate `cash-man`: done (2026-10-18, DONE.md);
   2. rendering: done (2026-10-18, DONE.md);
   3. `help` on the new pages: done (2026-10-18, DONE.md);
   4. the browser: done (2026-10-18, DONE.md), with `help`'s short view;
   5. the examples' test: done (2026-10-18, DONE.md);
   6. the first batch of about 20 pages at full depth, and drafts for the rest: done
      (2026-10-18, DONE.md); then a release;
   7. the next batches, about 20 pages each, each released when done.
2. **Document the two spellings a directory takes in `PATH`** (the user, 2026-10-18,
   after the side effects were weighed): no code. Spelling a `C:/` entry `/c/` as
   `PATH` is assigned was turned down, since `case ":$PATH:" in *":$HOME/bin:"*`, the
   idiom that adds a directory once, would then never find what it added. `$HOME` and
   `$PWD` are `C:/…` (D3) and inherited entries `/c/…` (D5): in spec §4 and
   `help differences`, with what a script that splits `PATH` on `:` should do. The
   finding:
   **A `C:/` directory put into `PATH` breaks a colon-separated split** (2026-10-18, found
   by the corpus's nvm case): `PATH="$PWD/bin:$PATH"` works for running programs, as
   cash's own search knows a drive, but the value keeps `C:/…` (D3's spelling of `$PWD`),
   so a script that splits `PATH` on `:` (`IFS=: read -ra`, `${PATH%%:*}`, awk with
   `RS=:` as nvm's `nvm_strip_path` does) gets `C` as an entry. D5 has scripts see `PATH`
   as `/c/…`, colon-separated, which a `C:/` entry assigned by a script breaks. In Git
   Bash `$PWD` is `/c/…`, so the list stays whole.
3. **zstd's real levels**: before the switch, its side effects are looked at: the size
   of `cash.exe` (a release stops past 2 MiB more), the port's unsafe translated code,
   and the memory of levels 19 to 22.
   **zstd compresses at ruzstd's fast level only** (parked by the user, 2026-10-17;
   phase 28, 2026-10-07): every level
   `-1` to `-22` gives about zstd's `-1`, larger than zstd's default `-3`. When
   `libzstd-rs-sys` (Trifecta Tech's port of libzstd) has a Rust API, the levels can be
   real; until then the page says so.
4. **The winget submission** (**yours** at the time): the manifests are validated
   (ROADMAP.md section 18); the submission is a pull request to `microsoft/winget-pkgs`,
   public, which goes out only with the user's yes then. Its side effects first: each
   release then needs a winget pull request of its own, and a winget install meets the
   first-start offer that changes the user's `PATH` and default Terminal profile.
5. **More corpus rounds**: permissively licensed real scripts and cases of cash's own
   on the Bash manual (`tests/corpus/SOURCES.md`).

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

- **An alias is not text: a quote it leaves open does not go on into the line**
  (2026-10-18, found writing alias's man page): with `alias q="echo it's"`, Bash reads
  `q quoted'` as `echo it's quoted'` and prints `its quoted`; cash stops with
  "unexpected EOF while looking for matching `''". Bash substitutes an alias's text and
  reads on; cash's parser takes it as words of its own. Rare in practice (an alias
  that opens a quote); the fix is in the tokenizer's alias expansion. Medium.

- **A `case` of more than 64 patterns with brackets or extended globs** compiles each
  pattern again on every pass: compiled patterns are kept in an LRU of 64, which a
  cycle through more misses every time. Patterns of characters, `?` and `*` match
  without a regular expression since 2026-10-18, which covers option parsers; brackets
  (`-[abc]`, `[Yy]*`) could be matched directly too, as Bash's strmatch does, if a
  script shows the need. Small to medium; the bracket rules must stay exactly the
  regular expression's (ranges by code point, ASCII classes, a lone `[` literal).

- **A benchmark tool in the repo for all five shells** (the user, 2026-10-18): cash, Git
  Bash, PowerShell 7 and WSL2's bash on Windows, and bash on native Linux, the same
  machine rebooted, so the results make one comparison. `tests/shell-bench` has most of
  it: `bench.sh` runs the workloads under hyperfine and already runs as plain bash inside
  WSL, so it runs on Linux as it is. What it lacks: an entry point for Linux (`run.sh`
  assumes Git Bash and `cygpath`), each run's results written into the repo
  (`tests/shell-bench/results/<date>-<os>/`, hyperfine's JSON and the machine: CPU,
  OS, shell versions), and `table.py` reading every results directory into one table. Not
  too complex: bash and hyperfine on both systems, Python only for the table. Small.

- **ROADMAP.md section 6 says cash's sed lacks `\b` and labels ended by a space**
  (2026-10-18, found checking the man page samples): both work now, as in GNU sed
  (`s/\bcat\b/dog/g`, `:a ;s/a/x/;ta`), and the corpus has three known differences, not
  four. The section's status paragraph is out of date. Small.

- **Three `cd` gaps against Git Bash 5.3** (2026-10-18, found writing the man page
  samples): `cd C:` sets `$PWD` to `C:`, where Git Bash goes to the drive's root
  (`/c`); `shopt -s cdable_vars` has no effect, so `cd src` with `src=C:/Windows` says
  "No such file or directory" where Git Bash goes to the directory the variable names; and
  `cd -@` says "not yet implemented" with status 99, where Git Bash refuses `-@` as an
  invalid option with its usage line and status 2. Small each.

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
