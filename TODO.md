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

## The phase now (2026-10-23): `figlet`, toward 2.0.0

The user's design, research/figlet-design.md, decided by pick lists on 2026-10-23
(decisions.md D84). One `figlet` builtin doing what figlet 2.2.5 and toilet 0.3 do, with
loose compatibility, 23 fonts built in, and extras. Released as **2.0.0** at the end, a
milestone and not a breaking change; the steps below are not released one by one. The
oracle is figlet and toilet in WSL's Arch (installed by the user, 2026-10-23).

1. `cash-figlet` (new leaf crate): fonts (`.flf`, `.tlf`, Latin-1 or UTF-8, code tags,
   colour escapes in glyphs), control files, the smusher copied from rs-figlet
   (Apache-2.0), wrapping, justification, paragraphs, direction, the canvas, the `text`
   export; rs-figlet's goldens.
2. The `figlet` builtin: figlet's options, the 23 built-in fonts, font search (`-d`,
   `FIGLET_FONTDIR`, `%LOCALAPPDATA%\cash\fonts`), zipped fonts, `-I`, help, man pages.
3. Colour fonts, toilet's options and filters, the `utf8` and `ansi` exports, `--color`.
4. The colour pack: gradients and palettes, lolcat rainbow, shadow and outline, Unicode
   borders, letter spacing, line height, background.
5. `figlist`, `showfigfonts`, and the font picker with a live preview.
6. Pixel fonts drawn with sub-cell blocks, any installed Windows font, double-size lines.
7. TheDraw `.tdf` fonts. (The sixel image mode was dropped by the user, 2026-10-23:
   too big for what it gives, and nobody would use it.)
8. The WSL oracle, the `cash doctor` shadow row, NOTICE and `upstreams.toml`.

## Next, straight after the figlet phase (the user, 2026-10-23)

The F1 help screen (`crates/cash-builtins/src/helpdocs/f1.md`, its MORE section):
- `man` above `help`;
- a line for `man` alone, the manual browser (a help browser), beside `man NAME`;
- check how the F1 screen shows in a window too small for it (narrow and short), and
  make it fit or degrade cleanly.

## The phase before (2026-10-23, done, released as 1.50.0)

The small leftovers (decisions.md, "The phase after 1.49.0"), each checked against Git
Bash 5.3 first: `find /dev/null`, a RETURN trap after a `.` that cannot open its file,
`eval` of a multi-line alias that unaliases itself, GLOBIGNORE with `:` inside brackets.
Then a corpus round of makeself, dehydrated, oh-my-bash and nb. Done, in DONE.md, with
`find -links`, tar's whole records, `command -v`'s path for a carried tool, `tput` for a
dumb terminal and a script waiting for its jobs inside cash (the user's picks); left,
below: job control in an interactive cash with no console.

## The phase before (2026-10-23, done, released as 1.49.0)

From "Found by the corpus round after 1.41.0" below (decisions.md, "The phase after
1.48.0"), each checked against Git Bash 5.3 first: **Traps, signals and jobs** whole;
the small items of **Scripts refused that Bash runs**, **Globbing and conditionals**,
**Starting cash** and **Builtins, small**; and the restricted shell. Done, in DONE.md,
with a `$BASHPID` for each copy of the shell (decisions.md) and F1 naming `man`; left,
below: a RETURN trap after a `.` that cannot open its file, a `set -m` script's finished
jobs before `wait`, GLOBIGNORE inside brackets and `find /dev/null`.

## The phase before (2026-10-22, done, released as 1.48.0)

The small leftovers (decisions.md, "The phase after 1.47.0"), each checked against Git
Bash 5.3 first: `(( A[\$k]++ ))` expanding `$k`, the two nameref cases, an unquoted
`${v-$@}` with `IFS=:`, a read-only assignment reported at line 1 of a script,
`declare -f` of `$( )` and `elif`, history numbers after HISTSIZE, `-ec -c`. Done, in
DONE.md, with `declare -f` words over lines, `set -H` in scripts, traces of lists,
`shopt -o`'s width and stable listing orders found on the way; left, below: the
nameref warning's count and `${v-$@}` under odd IFS values.

## The phase before (2026-10-21, done, released as 1.47.0)

Three themes (decisions.md, "The phase after 1.46.0"), in this order, each item checked
against Git Bash 5.3 first. Done, in DONE.md, but for what is left under the themes
below: an alias in `$(...)` read in POSIX mode, and `-ec -c`. `#!/bin/sh` running in
POSIX mode was put to the user and kept (decisions.md).

1. The leftovers of the themes done: arithmetic (single quotes, assignment's precedence),
   associative keys holding `]`, `[i]+=v` in a list, `${#/}`, the `EOF)` warning, the
   small builtins left (`hash -p`, `declare -f -a`, more usage errors), `set -a` with
   `declare`, `set -u` in `$(( a[0] ))`, `exec 0<file` in a script read from stdin.
2. Starting cash: `+B`, `+o braceexpand`, `-O badopt`, SHELLOPTS and BASHOPTS from the
   environment, `exec -a name` and BASH_ARGV0 as `$0`, the script operand on PATH, a
   renamed cash running `#!/bin/sh` with itself.
3. POSIX mode: POSIXLY_CORRECT, a special builtin's error, cash started as `sh`, `set
   -e` in a command substitution, a special builtin before a function of its name,
   aliases in `$(...)`, literal single quotes in `"${v-word}"`.

## The phase before (2026-10-21, done, released as 1.46.0)

The parser's remaining list (decisions.md, "The phase after 1.45.0"), each item checked
against Git Bash 5.3 first, cheapest first. Done, in DONE.md: 1, a trailing backslash
(backslash-newline inside backquotes was found to be as Bash has it); 2, `case x in
esac)` and loop names that are none; 3, text right after a list's `)` (a syntax error in
a list was found to end the line as Bash's does); 4, here-documents, but for the `EOF)`
warning (below); 5, `${ cmd; }`; 6, aliases: `foo>&2` was found to be as Bash has it,
and the self-unaliasing one is left (below); 7, functions printed that did not read back;
8, a syntax error inside `$(...)`, but for the end found twice (below).

## The phase before (2026-10-21, done, released as 1.45.0)

Taken as recommended while the user was away, as the user asked (decisions.md, "The phase
after 1.44.0"); the user may change it. The small builtins, each against Git Bash 5.3, in
this order. Done, in DONE.md: 1, `help` of the reserved words, and a plain word as a
prefix; 2, `complete -p`'s `!` and `compgen -V`'s name; 3, the usage errors; 4,
`history` with history off, `-d` ranges and `-p` failures; 5, `declare -f +r`.

## The phase before (2026-10-21, done, released as 1.44.0)

Taken as recommended while the user was away, as the user asked (decisions.md, "The phase
after 1.43.0"); the user may change it. In this order. Done, in DONE.md: 1, conditionals
(`=~` with a quoted part, `[[ -t X ]]`), but for `['a]']` (below); 2, globbing, but for
the few below; 3, `touch` going on past an operand it cannot touch; 4, the last phase's
small leftovers, `read -n -1` and `eval` and `source` holding their redirections, but for
`[i]+=v` in a list (it needs the list's words to carry `+=`, which they do not) and a key
holding `]` for `[[ -v A[$k] ]]` (both below).

## The phase before (2026-10-21, done, released as 1.43.0)

The user's pick from the themes below (decisions.md, "The phase after 1.42.0"), in this
order. Done, in DONE.md: 1, loop control (`break`, `continue`, `return`, `exit`); 2,
`read` with a mixed IFS and escapes, and `mapfile -d ''`; 3, `exec` and the redirection
state, but for `exec 0<file` in a script read from standard input (below); 4, `fc -0`, and
a script's history written as it ends; 5, the parser's cheapest subset; 6, readonly and
prefix assignments, `declare -g`; 7, `printf` but for a `TZ` with daylight-saving rules
(below); 8, expansion edge cases, but for the few below; 9, arrays and namerefs, but for
the few below.

## The phase before (2026-10-20, done, released as 1.42.0)

The user's pick after 1.41.0 (decisions.md, "The phase after 1.41.0"):

1. another corpus round, of sources the user picked: GNU Bash's own test suite, modernish's
   self-test, bash-it with its bats tests, and acme.sh's offline helpers (the downloads
   approved with the pick).

## Found by the corpus round after 1.41.0 (2026-10-20), not yet picked

Bash 5.3's own test suite (`tests/*.tests` against Git Bash 5.3, triaged by hand into
cash's bugs, Windows-caused differences and wording), modernish's self-test under cash, and
bash-it's bats files under `unixpaths`. Fixed in the round itself: the export listing of
Windows names that are no identifiers, `set -a` for every kind of assignment and for
functions, a function defined again keeping its attributes, `$BASH`, an inherited
`OLDPWD`, `alias` in POSIX mode, a subscript expanded twice (a command substitution in a
value ran), `history N` panicking, `#!/c/...` interpreters, `find -mmin` and its kin,
`getopts` taking `-h` for its own, `cash -ce`, `kill -sHUP`, a leading `~` or `#` left
unquoted by `set`, `printf %q` and `declare -p`, and `**/**` giving the directory's own
path. The rest, by theme:

**Scripts refused that Bash runs (parser)**
- The end of a `$(...)` is found twice, by the tokenizer and again by the word parser,
  each with its own `case` scan; one scan shared by both would end the mismatches (none
  known now). A syntax error in a backquoted substitution is said without Bash's
  `command substitution:` before it.
- An unterminated `$(` in a here-document is said as "failed to parse word" (Bash:
  "unexpected EOF while looking for matching `)'").

**Assignments, declarations, readonly**
- Namerefs: a reference named as its target in a function warns once (Bash: twice more
  each time it is used; only the count of warnings differs).

**Expansion and quoting**
- An unquoted `${v-$@}` is split as `$@` is (Bash, 2026-10-22: one word joined with
  spaces under `IFS=:`, the words kept whole under `IFS=': '`, a leading space under
  `IFS=x`; no one rule found). A quoted `"${v-$@}"` is as Bash has it.
- `${x@P}`: `\v` is cash's version, `\s` the script's name (by design).
- Bytes that are not UTF-8 become U+FFFD (`$'\xff'`, `read`, `%q`), and `$'\U'` above
  U+10FFFF.

**Traps, signals and jobs** (done in the phase after 1.48.0, DONE.md; left:)
- An interactive cash with no console (`cash -i -c ...` with pipes, oh-my-bash's corpus
  case) has job control on, `$-` holding `m`; Git Bash, which cannot take the terminal,
  has it off. Turning it off where no console is would take job control from cash under
  mintty, whose input is a pipe: to be looked at before changing.
- With `set -m` a script's job that ended before a plain `wait` is gone after it (Bash
  keeps it until `jobs` shows it as `Done`); one `fg` or `wait` collected leaves silently
  in both.

**printf, and times**
- A POSIX `TZ` with daylight-saving rules (`EST5EDT,M3.2.0/2,M11.1.0/2`) keeps standard
  time all year (`timefmt::Zone::Fixed` drops the rules; a name chrono-tz knows,
  `EST5EDT`, works): `printf %(...)T`, `date`. A rule engine for `Mm.w.d/time`, `Jn` and
  `n` is a medium job, for a `TZ` rarely set on Windows.
- Widths and precisions above INT_MAX, and `%(abde)Z`'s warning, are said otherwise.

**POSIX mode**
- An alias inside `$(...)` is expanded as the substitution runs, not as it is read
  (Bash in POSIX mode: a function defined before the alias does not see it); it needs
  the substitution's text rewritten as it is parsed, much work for a rare case.

**Builtins, small**
- `eval 'break 1 2'` and its kin inside a subshell end it with 2 where the text was
  sourced or `eval`ed (Bash: 1).
- Printing: associative arrays in sorted order, and `complete -p` by name (Bash: hash
  order for both).

## The phase before (2026-10-20, done)

The user's pick after 1.40.0 (decisions.md, "The phase after 1.40.0"):

1. another corpus round, of sources the user picks: bashdb, bash_unit and bats-file,
   composure, direnv's stdlib: done 2026-10-20 (DONE.md), with `trap` and `kill -l`
   knowing all of Linux's signals (bashdb, D11), `man -w` (direnv), and a process
   substitution's directory written down as one that cannot be entered (bash_unit, D17).
2. a `cd ..` climb that never ends (found in item 1, direnv's `find_up`): under
   `unixpaths`, a climb past a drive's top spells `$PWD` `/` (D83,
   research/root-directory-design.md), after directories that exist only in cash were
   built, weighed and reverted unreleased: done 2026-10-20 (DONE.md).

## The phase before (2026-10-20, done)

The user's pick after 1.39.0 (decisions.md, "The phase after 1.39.0"):

1. the DEBUG trap where Bash runs it (found in the phase before): `[[ ]]`, `(( ))`,
   `for`, `case`, `select`, a function's start and each stage of a pipeline in the shell
   itself; bats-assert's twin then matches Bash: done 2026-10-20 (DONE.md), with
   `extdebug`'s skips, and a trap keeping `PIPESTATUS` and an ignored failure (found in
   it).

## The phase before (2026-10-20, done)

The user's pick after 1.38.0 (decisions.md, "The phase after 1.38.0"):

1. build the `/c/` mode of D82 (research/unix-paths-mode-design.md): a shell option,
   inherited by a child cash; paths shown as `/c/...`; whole-word `/c/...` and
   `--opt=/c/...` arguments, and `HOME`, `PWD` and `OLDPWD`, turned into `C:/...` for
   Windows programs that are not MSYS ones; the corpus's climbing tools and bats-assert run
   unchanged in the mode: done 2026-10-20 (DONE.md), with the bundled tools reading `/c/`
   in either mode (the user's pick, found in it), and five bugs found in it fixed.

## The phase before (2026-10-20, done)

The user's picks after 1.37.0 (decisions.md, "The phase after 1.37.0"):

1. a corpus round and a stress round: nvm as a whole, the bash-completion library,
   pyenv with python-build, and bats-assert with bats-support, the downloads approved by
   the user's pick of the sources: done 2026-10-20 (DONE.md);
2. `type -P` gives a bundled tool as `/usr/bin/NAME` (the user's pick, found in item 1):
   done 2026-10-20 (DONE.md);
3. a design for an opt-in `/c/` mode, in research/, then a pick list: done 2026-10-20 (the user's pick,
   found in item 1): `$PWD`, `pwd` and `HOME` as `/c/...`, and `/c/` arguments turned into
   `C:/` for Windows programs, so that tools climbing to `/` (rbenv, asdf, pyenv, nvm)
   and bats' `load` work. To weigh in it too: `type -P` of a program cash does not carry
   gives its Windows path, under `C:/Program Files` for Git's own (`gawk`), whose space
   splits an unquoted `$(type -P gawk)`, where Git Bash gives `/usr/bin/gawk`
   (`pyenv help`). bats-assert's corpus case is known to differ until then. Designed
   2026-10-20 (research/unix-paths-mode-design.md); the user's picks are D82, to be built
   in the next phase.

## The phase before (2026-10-20, done)

The user's picks after 1.36.0 (decisions.md, "The phase after 1.36.0"), in this order:

1. an `EXIT` trap run by `exit` inside a function: done (DONE.md);
2. another corpus round: todo.txt-cli, git-quick-stats, git-subtree and bash-preexec, the
   downloads approved by the user's pick of the sources: done 2026-10-20 (DONE.md).

## The phase before (2026-10-20, done)

The user's picks after 1.35.0 (decisions.md, "The phase after 1.35.0"):

1. another corpus round: git-prompt.sh, bash3boilerplate and sharness, the downloads
   approved in the same pick: done 2026-10-20 (DONE.md).

## The phase before (2026-10-20, done)

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

- **Three empty files, `1`, `f` and `file`, at the repository's root** (untracked, made
  2026-10-19 between 07:33 and 07:37): from no test that was found; perhaps a command
  typed with a stray `>`. Left in place, as another session may own them.

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
