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

## Shared parts for the builtins (the current phase, 2026-10-16)

The user's choice (decisions.md, "Builtins share their option parsing, input opening and
folder walks"; `research/central-components-design.md` has the surveys and the design).
One tool at a time, each against its tests and Git Bash; its messages stay its
upstream's.

1. **Options**: done (2026-10-17, DONE.md).
2. **Inputs**: done (2026-10-17, DONE.md).
3. **Walks**: done (2026-10-17, DONE.md).
4. **The parts called most**: done (2026-10-17, DONE.md; `research/hot-paths.md`).

## Left over from finished phases

What the phases now in DONE.md left open, each where its phase says more.

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

- **The ConPTY tests that run PowerShell time out in CI now and then**: PowerShell
  started by the test prints nothing in 30 s (`conpty_interactive_tests.rs:220`).
  Run 37953062174 (2026-10-09, slow lane 1/4, both tries):
  `conpty_a_program_that_ran_leaves_a_healthy_prompt_where_it_was`, on a runner whose
  setup also logged a "bash startup failure". Run 37963172593 (the same day, slow lane
  4/4, both tries): `..._leaves_a_scroll_region_...`, `..._dies_in_full_screen_...` and
  `..._had_vt_input_on_does_not_eat_keys_typed_ahead` (line 402). Runs between passed;
  the commits touched only rar. Look at PowerShell's start on the runner (a first-run
  cache?) before raising the wait.
- **A benchmark of 1.10.0's compressors** (the user asked, 2026-10-07; best of two runs,
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
- **GNU ls options cash's ls has not** (2026-10-17): named as not supported: `-Q`,
  `-n`, `-s`, `-i`, `-c`, `-u`, `--time`, `-H`, `--quoting-style`, `-b`, `-q`, `-N`,
  `-k`, `--si`, `--block-size`, `-T`, `--zero`, `--hyperlink`, `-Z`, `--author`, `-D`.
  The common ones are done (DONE.md).
- **ls's words for a bad `--color` or `--classify`** (2026-10-17): `Valid arguments
  are: 'always', 'auto', 'never'`, status 2, where GNU's ls lists them three to a line
  (`'always', 'yes', 'force'` and so on), status 1, as cash's `--format` now does.
- **The clipboard test fails while another program holds the clipboard** (2026-10-16):
  `clipboard_and_small_tools::pbcopy_and_pbpaste_round_trip_through_the_clipboard` reads
  the clipboard first and fails with "the clipboard is in use by another program" on a
  busy desktop (a clipboard manager, an app copying). A short retry of the open, in
  `cash_win32::clipboard` or in the test, would ride it out.
- **Work done again on every call, from the 2026-10-16 survey** (after `ls` was found
  re-reading Windows Terminal's settings per listing; the session facts now have one
  place, `cash_win32::facts`). Still worked out each time:
  - `which <builtin>` and `type -a` scan all of PATH on every call;
  - the completion menu runs the PATH scan (or carapace) again on every keystroke while
    it is open;
  - a name not found is not remembered, so each miss probes PATH twice;
  - a script's shebang interpreter is looked up twice, past the hash table;
  - each `.ps1` run scans PATH for pwsh;
  - `HISTCONTROL=erasedups` scans the whole history (50,000 entries) after each command;
  - `Shell::clone` copies the builtins table for every subshell;
  - coolfetch asks sysinfo for everything on each run, nothing kept between runs.
- **What the upstreams cash took in have done since** (2026-10-16, `cargo xtask
  upstreams`; the weekly Upstreams workflow keeps an `upstreams` issue): uutils
  diffutils 6 commits since the revision taken, uutils sed 34, posixutils-rs 42 in
  `awk/` and 8 in `calc/` (bc). Each wants looking through, porting what matters and
  marking the rest `reviewed` in `upstreams.toml`. sevenz-rust2 0.24.0 was looked
  through: nothing to port.
- **reedline 0.52's fixes, against cash-reedline** (taken from 0.51.0): panics on a vi
  count past `u16::MAX`, an empty `UntilFound` keybinding, a failed history search in
  the hinter, and in the menu's selection parsing, among about twenty fixes. cash had
  already fixed the `SubstringSearch` `todo!()` (`engine.rs`); the rest want comparing
  one by one.
- **The archivers finish their run before a Ctrl-C is acted on** (2026-10-16): `tar`,
  `zip`/`unzip`, `7z` and `rar` reading files by name do not ask their
  `cash_core::StopToken`; their standard input does stop. Their walks of folders do
  since 2026-10-17, and fail the run (tar and zip in silence, rar "User break", 7z "Break
  signaled"); what is left is their member loops and reads. A second Ctrl-C no longer
  ends the shell under two seconds. Their reads must go through `StopToken::reader`,
  which fails, never an end that looks like the file's: an end made gzip finish half an
  archive and remove the file.
- **A Ctrl-C or `kill` on a large `xz` waits for its blocks** (2026-10-16): lzma-rust2's
  threaded encoder takes the whole input at once (13 blocks of 24 MiB from a 300 MB
  file, on 16 threads) and writes the first block only when it is done, about 12 s
  later; a block cannot be stopped half way, so the stop is acted on at that first
  write (`StopToken::writer`). Nothing is lost: the archive is abandoned and the file
  kept. Acting at once would need the coding on a thread of its own, left to wind down
  writing nowhere, or a stop inside lzma-rust2's block loop.
- **`{ head -n1; cat; } < file` loses what head read ahead** (2026-10-16): uutils' `head`
  seeks the shared input back on Unix only (`uu_head` 0.12.0 `head.rs:460-489`); GNU head
  leaves a seekable input just after what it printed.
- **spec.md §8 says an in-process `rm` could not fix §4 #13** (2026-10-16): it could, as
  cash's process never sits in the shell's folder (D10); the text wants correcting.
- **The DEBUG trap misses a command that only assigns** (2026-10-16): `trap '...' DEBUG;
  x=1` runs the trap before `x=1` in Bash, not in cash, which fires it only for a
  command with a name (`commands::on_preexecute`).
- **A write to a closed standard output says too much** (2026-10-16): `echo hi >&-`
  says `echo: i/o error: I/O write error: standard output not available`, where Bash
  says `echo: write error: Bad file descriptor`; both exit 1.
- **zstd compresses at ruzstd's fast level only** (phase 28, 2026-10-07): every level
  `-1` to `-22` gives about zstd's `-1`, larger than zstd's default `-3`. When
  `libzstd-rs-sys` (Trifecta Tech's port of libzstd) has a Rust API, the levels can be
  real; until then the page says so.
- **`printf` takes 1.4 times Bash's time** (2026-10-17, `research/hot-paths.md`): 6.3 µs
  a call where `true` takes 2.2; its arguments change from call to call, so clap's
  parse cannot be remembered as `read`'s and `declare`'s are, and uucore parses the
  format on every call. A hand-made parse of its one option (`-v`) would take clap's
  part, with clap's errors kept in Bash's words.
- **The parse caches are 64-entry LRUs behind a global lock** (2026-10-17): words,
  tokens, patterns and `eval`'s programs (the `cached` crate); each hit copies its key and
  its value, and past 64 distinct words in a loop a word costs 1.8 µs against 1.3. An
  `eval` in a loop copies its whole program on each hit. A thread's own map, larger, as
  arithmetic's `parse_shared` has, would take neither the lock nor the key's copy.
- **`cd` takes 62 µs** (2026-10-17): `std::fs::metadata` opens a handle to tell a folder,
  where `GetFileAttributesExW` would not, and the path is rendered twice. Bash's `cd`
  under MSYS takes 1.1 ms, so it is not behind, but a `cd` in a loop pays it.

---

## Decided, written down so it is not decided twice

In [decisions.md](decisions.md), by topic (moved 2026-10-11); new decisions go there.
