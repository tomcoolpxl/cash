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

## Picked by the user (the current phase, 2026-10-20)

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

**Scripts that end or go on in silence**
- `break`/`continue` outside a loop, or `break N` past the loops, ends the script with
  status 0; a count of 0, below 0 or no number loops on; inside a function called from a
  loop: "not yet implemented", status 99; `return -1` refused (Bash: 255), `return abc`
  does not return.
- `exec` with a redirection inside a redirected `{ }` or function makes the outer
  redirection permanent: what follows lands in the file.
- `exec {v}<<EOF` and `exec {v}<<<word` run `{v}` as a command, status 127.
- A failed `exec`: `shopt -s execfail` ignored; `exec .` and `exec notthere` go on;
  `exec -1`/`exec -i` taken for commands, the script ended (Bash: invalid option, status 2).
- A background command whose input is a here-document or a pipe, inside a block or
  subshell, reads `/dev/null` (`echo x | ( cat & wait )` prints nothing).
- A failed redirection on a compound command ends an enclosing `( )`, and the ERR trap
  never runs for it.
- A shell reading its script from standard input ignores `exec 0<file`.
- `{ exec 8</dev/null; } 8<&-` leaves fd 8 open after the block (modernish).
- `( ! exit 42 )` is 0 (Bash: 42).
- `fc -0` and `fc -s -0` run the last command again (Bash: out of range).
- `set -o history` in a script writes every command to `$HISTFILE` at once; Bash writes
  nothing.

**`read` and `mapfile`**
- `IFS=": " read x y` keeps the delimiters in the last variable (`a :b` gives `:b`);
  832 of Bash's 6856 generated IFS cases.
- A backslash-escaped trailing space is dropped; with `IFS=$'\t\r\f\v'` trailing IFS
  white space is kept; `mapfile -d ''` keeps the NUL in each element.

**Scripts refused that Bash runs (parser)**
- `! time cmd`, `time` after `!` or `time`, `time --`.
- `$(case x in in|esac) ...;; esac)`: the end of `$(` is found wrong around reserved words
  in patterns; a syntax error inside `$(...)` is found only when it runs.
- A here-document in `$(...)` ended by `EOF)`; `<<-'<TAB>END'`; the delimiter checked
  before backslash-newline is joined (`EO\`+`F`); an unterminated `$(` in a here-document.
- A trailing backslash at the end of input is dropped; backslash-newline inside
  backquotes is kept.
- A syntax error in a compound array assignment ends the script; text after its `)`
  (`let a=(4*3)/2`, `c=(x y)z`); a brace range across the backtick (`{Z..a}`).
- `case x in esac)` accepted; `for 1 in a b` and `select` run with names that are none.
- `${ cmd; }`: `return`, `local`, `exit`, a function inside it, `}` after a newline or `)`.
- Aliases: one followed by a redirection merges tokens (`foo>&2`); `eval` of a multi-line
  alias that unaliases itself; in POSIX mode, aliases inside `$(...)` at parse time and a
  reserved word after a blank-ending alias.
- Functions printed that do not read back: a here-document in an `if` condition, `export
  -f` of one with a here-document and a later redirection, names that are no identifiers.

**Assignments, declarations, readonly**
- A prefix assignment before a declaration builtin is ignored (`var=60 export var`, `x=4
  declare -r x`, `z=y typeset z`); in POSIX mode one before a special builtin does not
  last (`y=5 eval :`); `local` does not take a prefix assignment's value (`a=4 f`).
- Readonly not enforced: an element of a readonly array assigned or unset; `local var`
  over a readonly global; `coproc RO`; a prefix assignment to a readonly variable runs the
  command in silence.
- `declare -g` in a function writes the caller's local; `declare name[sub]=v` writes
  element 0; `a+=5 cmd` assigns; `[i]+=v` in a compound assignment.
- Associative lists: unquoted words split and globbed, `( [k]=v one )`, an empty key;
  keys holding `]` or quote characters break `(( ))`, `[[ -v ]]`, `unset`, `read`,
  `printf -v`; `(( A['$key']++ ))` expands `$key`.
- `(( a[i++]++ ))` evaluates the subscript twice; bad or negative subscripts (`b[]=x`,
  `[-65]=v`, `x+=([-1]=v)`) land on element 0; `declare +a scalar` makes an array.
- A failed indexed assignment leaves the variable unset; `x+=7` on a bad old value.
- `declare -a` with no names prints `name=(...)`, not `declare -a name=(...)`.
- `set -a` does not export a variable reassigned by `declare`; `set -k`; `shopt -s
  localvar_inherit`/`localvar_unset` do nothing; `export -a` refused; `set -u` misses an
  unset element in `$(( a[0] ))`.
- Namerefs: `declare +n`; attributes go to the reference, not its target; `for` over a
  reference; targets unchecked (`declare -n r=/`, a reference to `X[0]` used by `read`);
  a reference named as its target in a function; `${ref=X}`.

**Expansion and quoting**
- Quotes inside a double-quoted `${v-word}`: `"${v-"'x'"}"`, `"${v-'"'x}"` (fails),
  `\}`, a tilde inside nested quotes, `${A:=PATH=~/bin}`; POSIX mode's literal single
  quotes; `$'...'` inside `${...}` in double quotes, and in a here-document.
- `"${foo:-$@}"` with no parameters is no word (Bash: one); `"$x$@"` is one (Bash: none);
  `$*` in a pattern joins with a space, not IFS's first character; `${v-$@}` with `IFS=:`.
- Pattern substitution: an empty pattern matches, `${e/#/x}`, `${a///a/}`; the offset
  `${P:1 ? 4 : 2:1}`; `${!9:-d}`, `${!n[@]@Q}`, `${@@A}`; `${#/}` accepted.
- Arithmetic: assignment binds tighter than `?:`, `&&`, `||`; `++7` and `--7` refused;
  `$(( 'foo' ))` accepted; a variable holding `b[$d]` not expanded.
- `${x@P}`: `\v` is cash's version, `\s` the script's name.
- Bytes that are not UTF-8 become U+FFFD (`$'\xff'`, `read`, `%q`), and `$'\U'` above
  U+10FFFF.

**Globbing and conditionals**
- Quotes and backslashes inside a glob pattern are not honoured (`a[\;]b`, `a["b"]c`,
  `pat='ab\cd*'; echo $pat`).
- `**/` leaves out a link to a directory, which `*/` lists.
- Extglob with dot files (`@(.foo)`); `*` gives `.` and `..` with `globskipdots` off; a
  malformed bracket inside a group.
- Collating symbols and equivalence classes (`[[.a.]]`, `[[=a=]]`); invalid class names
  match; GLOBIGNORE splits on `:` inside brackets; nullglob with `/` in brackets.
- `[[ $s =~ "lit" ]]` leaves BASH_REMATCH empty; `[[ a =~ ['a]'] ]]` matches; `[[ -t X
  ]]` is silent; POSIX mode globs a redirection's word.

**Traps, signals and jobs**
- `kill -SIG $$` from a background job runs the trap in the job, not the shell.
- `return` in a trap does not return from the function it interrupted.
- `$!` is not set by a process substitution.
- RETURN and DEBUG traps see the wrong LINENO and a `source` frame; `caller` at the top
  prints nothing; BASH_COMMAND after a function ending in `[[ ]]`.
- `fg` with monitor off, `%1` as a command, `fg %name`, `bg`; `[N]+ Done` printed by a
  script with `set -m`.

**printf**
- `\c` ends the output (Bash prints it); `\'` and `\?`; width or precision on `%b` and
  `*` on `%q`/`%b`; `%(...)T` with nested parentheses or a TZ with DST rules; `%ls`/`%lc`
  count bytes; `%S`/`%C`; `\u` with fewer than four digits; text before a bad conversion
  is lost; widths above INT_MAX.

**POSIX mode**
- `POSIXLY_CORRECT` does not turn POSIX mode on or off, nor does the mode set it
  (modernish reads `export -p` so).
- A special builtin's error does not end the shell (a redirection on `:`, `unset` of a
  readonly, `return` outside a function, `readonly` of a bad name); `trap ... SIGNOSIG`
  and `command eval "( "` end it, where Bash goes on.
- cash started as `sh` is not in POSIX mode; a command substitution does not inherit
  `set -e`; a special builtin does not win over a function of its name.

**Starting cash**
- `+B`, `+o braceexpand`, `-O badopt`; exported SHELLOPTS and BASHOPTS not read, and not
  read-only; `exec -a name "$BASH"` and an imported BASH_ARGV0 do not set `$0`; the
  script operand is not looked for on PATH; `--pretty-print`.
- A cash renamed (`bash.exe`) sends `#!/bin/sh` scripts to the `cash` on PATH, not to
  itself (`resolve_named_interpreter` asks for the stem `cash`).

**Builtins, small**
- `complete -p` drops the `!` of `-X '!pat'`; `complete -r` and `compgen -V` of a bad
  name are silent; `help case` and `help -s rea`; `hash -p` ignored; `declare -f +r` and
  `-f -a`; `set -r` restricts nothing; `history` does nothing while history is off,
  numbers restart after HISTSIZE, `history -d 2-3`, `history -p '!!:z'`, script lines not
  history-expanded under `set -H`; `find /dev/null` (modernish's find check); about
  thirty missing usage errors.
- An assignment to a read-only variable is reported at line 1 of a script, whatever its
  line (bash-it's case, line 10 in Bash).
- Printing: `declare -f`/`type` (`|cat`, `2>& 1`, `$( )`, `elif`), xtrace of a compound
  assignment and inside traps, `shopt -o` column width, `complete -p` and `compgen -b`
  order changing between runs, associative arrays in sorted order (Bash: hash order).

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
