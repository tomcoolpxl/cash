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
  nextest run (`--profile full --retries 0`, both lanes); then `main` is brought up to the branch and pushed,
  the phase is released once CI is green, and a short report is written. The next phase
  starts without waiting. The work stops only for a decision marked **yours** below, or
  for a release whose CI fails.
- **Releases:** each phase ends with a release of its own. A tag is made only after CI
  has passed on the commit (RELEASING.md). So far: 1.3.0 is what was on `main` on
  2026-09-30, 1.3.1 the test suite (phase 1), 1.3.2 Ctrl-C everywhere (phase 2), 1.3.3
  process substitution (phase 3), 1.3.4 the `/dev` names and the bundled tools (phase 4),
  1.3.5 jobs, `wait` and the tools beside them (phase 5), 1.3.6 the Tab menu opening on
  the history hint's candidate (asked by the user on 2026-10-02, outside the phases),
  1.3.7 the shared part on the first Tab and the grid on the second (phase 7), 1.3.8
  crashes, security and CI hardening (phase 8), 1.3.9 the shell's own working directory,
  environment, PATHEXT and umask (phase 9), 1.3.10 awk, sed and bc (phase 10), 1.3.11 pipelines, jobs and process
  substitution (phase 11), with the two parser bugs phase 11 found (`( ( … ) )` and
  `--file=<(…)`), fixed while CI was put right, 1.3.12 the language (phase 12), 1.3.13
  builtins and error output (phase 13).
- An item says what was seen and what Bash does; a cause only where it was looked for.
  Longer notes on a thing that stays open belong in `open-issues.md`.

Collected on 2026-09-30 from every cash session of that day and the day before. All of
their work is in `main`; the sessions themselves are not needed any more. The order and
the grouping were chosen that day: the test suite first, so that every later phase can
trust its runs, then by what a user notices most.

Phases 1 to 13 are done. Phase 6 changed no code and had no release of its own.

Phases 8 to 14 are the code review of 2026-10-02. `REVIEW_REPORT.md` has the evidence
and the reasons; the IDs (EXE-01, TXT-01, …) are its findings, and R1 to R11 its
recommendations. Each item was reproduced against Git Bash 5.3 during the review unless
it says "from reading". Items that share a fix are one item. The order was chosen by the
user on 2026-10-02: what can hurt a user first, then what removes a whole class of bugs,
then the riskiest changes once the crashes and state bugs are out of the way.

| Phase | Release | What |
| --- | --- | --- |
| 8 | 1.3.8 | Crashes and security, and CI hardening |
| 9 | 1.3.9 | The shell owns its state |
| 10 | 1.3.10 | awk, sed and bc |
| 11 | 1.3.11 | Pipelines, jobs and process substitution |
| 12 | 1.3.12 | Language |
| 13 | 1.3.13 | Builtins and error output |
| 14 | 1.3.14 | Tests, records and leftovers |
| 15 | 1.3.15 | GNU parity of the bundled sed and awk |
| 16 | 1.3.17 | sudo, su and `cash help` |
| 17 | 1.3.18 | `lsof` lists everything |
| 18 | 1.3.18 | `ss` as iproute2 7.2 has it |
| 19 | 1.3.19 | `ss -i` and `dev`, and the vi fix |
| 20 | 1.4.0 | `croot`: a built-in file and folder picker on Alt-E |
| 21 | 1.5.0 | Console, clipboard and the remaining small tools |
| 22 | 1.6.0 | A per-user installer, `cash --update`, and winget |
| 23 | 1.7.0 | `grep`, `diff` and `cmp`: the standing rule reversed |
| 24 | 1.8.0 | `gzip`, `gunzip` and `zcat` |
| 25 | 1.8.0 | Compatibility corners: `/dev/tcp`, `command_not_found_handle`, persistent `abbr`, kinder refusals, `getconf`, `locale` |

---

## Phase 16. sudo, su and `cash help`

Found writing the help catalogue (2026-10-04); all fixed, released as 1.3.17.

---

## Phase 17. `lsof` lists everything

Chosen by the user on 2026-10-04, after 1.3.17:

Bare `lsof` and the open files of `-p`/`-c`/`-u` come from the handle walk
(`cash_win32::handles`, spec D50) since 2026-10-05, and so do `+D`, `+d` and a folder
named: `lsof +D /tmp` over `TEMP`'s 87,411 folders took over two minutes asking the
Restart Manager file by file, and takes under two seconds. Done.

---

## Phase 18. `ss` as iproute2 7.2 has it

Compared with iproute2 7.2.0's `ss` in WSL and its `misc/ss.c` on 2026-10-05; done the
same day. The default view, `state`/`exclude`, the state names, the filter grammar's
errors, peerless sockets, `-s` with a selection, IPv6 scope names, getopt_long parsing,
`-A !table`, the Linux-only long options, row order, `-V` and `-F` follow iproute2; `-r`,
`-K` and `-B` were added, as the user chose. As in iproute2, whose `SS_CONN` keeps the
bound-inactive bit, plain `ss -t` lists bound sockets as `UNCONN`.

Left as differences (spec D51, row 29): a dual-mode socket is two lines, not one `*`;
`-K` closes IPv4 connections only and only elevated; `-K`'s elevated path has not run
on the author's machine (the test runs it when elevated).

Not chosen then: `dev NAME` and `-i` (both taken up in phase 19), `-m` (`GetPerTcpConnectionEStats`
needs an administrator to switch collection on per connection), `-E` (polling only).

---

## Phase 19. `ss -i` and `dev`, and the vi fix

Asked for by the user on 2026-10-05; released as 1.3.19 with the vi-mode fix (reedline
patch 8):

- `ss -i`: per-connection TCP statistics from `GetPerTcpConnectionEStats`.
- `ss ... dev NAME`: matches the IPv6 scope's interface; a socket without one has no
  device, as a Linux socket without `SO_BINDTODEVICE`.

---

## Phase 20. `croot`: a built-in file and folder picker on Alt-E

The user used broot on Alt-E for two weeks (set up 2026-09-27 in `~/.bashrc` and broot's
`cash-cd.hjson`) and wants it built in, as a picker only: no search language, no
editor integration, no verbs. Broot takes up to two seconds to open, being a separate
program; built in, the first frame comes from one folder read. Chosen on 2026-10-05:

- **Inline, below the prompt** (as fzf's `--height`): 40% of the window, at least 8
  rows, `CASH_PICKER_HEIGHT=50%` or `=15`; the scrollback above stays visible, the
  screen scrolls up when the prompt is near the bottom, a tiny window gets full screen.
- **Broot's tree**: several levels open, trimmed to fit with `… N more`; Right makes the
  selected folder the root, Left goes to the parent; at a drive's root, Left shows the
  drives (C:, D:, mapped drives, `~`).
- **Typing is a fuzzy filter that also searches deeper**, in the background, best
  matches first, capped in time and entries; Esc clears it, then closes.
- **Context from the command line**:
  - folders only for an empty line, `cd`, `pushd`, `rmdir`, `mkdir -p`; folders and
    files for any other command; Alt-F switches;
  - the word at the cursor, when it is part of a path, sets the start (`cd ~/src/fo`
    starts in `~/src` filtered by `fo`) and is replaced by the pick;
  - an empty line, `cd` or `pushd`: the pick goes in and runs at once; any other
    command: the pick is inserted with a space after it and nothing runs;
  - a command that takes several paths (`cp`, `mv`, `diff`, …) keeps the picker open
    for the next argument; one known to take one closes it; unknown commands stay open.
    Esc, or picking with Ctrl-Enter, closes.
- **Path form**: relative below the current folder, `~/…` under home, else `C:/…`;
  quoted only when needed; folders end in `/`.
- **Extras in v1**: drives above `C:/`; a folder-history view (Alt-H) from cash's own
  folder history (`cdh`); hidden entries (dotfiles, the hidden attribute) and
  .gitignored ones left out by default, Alt-. and Alt-I show them.
- **Colours**: entries as `ls` colours them (`LS_COLORS`); the picker's parts from
  `CASH_PICKER_COLORS` (`sel=…:match=…:frame=…`).
- **Keys**: Alt-E, as a Readline function (`cash-picker`) that `bind` can move; it
  replaces the broot binding in `~/.bashrc`. No mouse in v1.
- **Also a builtin, `croot [-f|-d] [DIR]`**: the same picker, printing the pick(s) on
  standard output for scripts (`cd "$(croot)"`, `vim $(croot -f)`).

Spec D73, approved by the user on 2026-10-05, is the full description. Fuzzy
matching and .gitignore from established crates (e.g. `nucleo-matcher`, `ignore`),
not hand-written.

Released in 1.4.0 (2026-10-05): the `cash-picker` crate (line context, picks, tree, filter,
deep search, colours, the picker's keys and frames, the terminal runner), Alt-E in the
line editor (`cash-picker`), the `croot` builtin, `help croot`; ConPTY tests for a `cd`
pick and an inserted file. The broot block is out of the user's `~/.bashrc`, at their
request (backup `~/.bashrc.before-croot`).

After the user's first use: 1.4.1 lets Down and PgDn scroll through a long folder,
1.4.2 shows each pick on the command line while picking, 1.4.3 stops the flicker while a
search runs. Then (2026-10-06): the first frame measured at a median of 17-19 ms
(`tests/croot_latency.rs`, spec D73), a resize works the height out again, and Alt-S
sorts newest first with each entry's age. Nothing left open.

---

## Phase 21. Console, clipboard and the remaining small tools

Asked by the user on 2026-10-06, after a second look at what a bare Windows machine with
cash still lacks (`research/busybox-gap-analysis.md` has the first look). Every name
below resolves to nothing on a clean Windows install; several resolve to Git for Windows'
`usr/bin` or a Scoop shim when those are on `PATH`, and those stay reachable by path,
as `type -a` shows. No Windows program in System32 has any of these names, so none of
them hides one. The tools, in the order they are built:

1. **`tput`** (ncurses 6.6's for `xterm-256color`): the string, numeric and boolean
   capabilities scripts use (`setaf`, `setab`, `bold`, `sgr0`, `smul`, `rev`, `cup`,
   `civis`, `cnorm`, `sc`, `rc`, `el`, `ed`, `clear`, `smcup`, `rmcup`, `cols`,
   `lines`, `colors`, …), written as VT sequences without terminfo, as `clear` and
   `reset` already are; `cols` and `lines` from the console, also when standard output
   is a pipe; `-S` from standard input; `-T` accepts xterm- and VT-like names only;
   ncurses' exit codes (an unknown capability is 4, a false boolean 1).
2. **`stty`** (GNU coreutils 9.11's words): `-echo`/`echo`, `raw`/`cooked`/`sane`,
   `-icanon`, `isig`, `size`, `-a`, `-g` and restoring from its string, `rows`/`cols`;
   each setting mapped to the console modes cash owns (echo, line input, processed
   input); settings Windows has no counterpart for (`erase`, `intr`, `ixon`, speeds)
   are accepted so scripts run; `-F FILE` refused. Cash puts the console back before each
   prompt, so a change lasts for the current command or script.
3. **`iconv`** (glibc's options): `-f`, `-t`, `-l`, `-c`, `-s`, `-o`, `//TRANSLIT` and
   `//IGNORE`; every code page Windows has (`CP437`, `CP850`, `CP1252`, `LATIN1`,
   `ISO-8859-*`, `KOI8-R`, `SHIFT_JIS`, `GBK`, `BIG5`, `EUC-KR`, …) through
   `MultiByteToWideChar`, plus UTF-8, UTF-16LE/BE and UTF-32LE/BE with BOM handling;
   glibc's "illegal input sequence at position N" and status 1; the default charset
   is UTF-8.
4. **`column`** (util-linux 2.42.3): `-t`, `-s`, `-o`, `-c`, `-x`, `-n`, `-L`, `-N`,
   `-R`, `-H`, `-J`; widths in display cells; a CRLF line stays CRLF. `-T`, `-W` and
   `-E` are refused by name.
5. **`xxd`** (vim's): `-p`, `-r`, `-i`, `-l`, `-s`, `-c`, `-g`, `-u`, `-b`, `-e`, `-o`,
   `-C`, `-n`, `-d`, `-a`, `-R`; no colour unless `-R always`; `-E` refused.
6. **`hexdump`** (util-linux 2.42.3): `-C`, `-c`, `-d`, `-o`, `-x`, `-b`, `-n`, `-s`,
   `-v`, and `-e FORMAT` with the format language (`_a`, `_A`, `_c`, `_p`, `_u`).
7. **`uuidgen`** (util-linux): `-r` (the default), `-t`, `-m`/`-s` with `-n` and `-N`
   (`@dns`, `@url`, `@oid`, `@x500`), `-x`, `-C`.
8. **`xdg-open`**: `start` under the name cross-platform scripts try first; xdg-open's
   exit codes (1 syntax, 2 no such file, 4 the handler failed).
9. **`pbcopy`** and **`pbpaste`**: the clipboard as Unicode text. `pbcopy` reads UTF-8
   and turns lone LF into CRLF, so Windows programs paste it right; `pbpaste` writes UTF-8
   with CRLF turned into LF, so `pbcopy < f; pbpaste | diff f -` is quiet. A clipboard
   holding no text gives nothing, status 0. `clip.exe` stays as it is: it writes the
   console code page, and Windows has no paste command.
10. **`watch`** (procps-ng 4.0.7): `-n`, `-t`, `-d[=permanent]`, `-e`, `-g`, `-c`,
    `-x`, `-b`, `-p`, `-r`, `-w`; the command runs through cash each time (procps runs
    `sh -c`, which is cash anyway), on the alternate screen; `q` and Ctrl-C end it;
    procps' exit codes.
11. **`free`** (procps-ng 4.0.7): `-b -k -m -g -h --si -t -w -s N -c N -l`; `Mem:` from
    the same call `top` uses; `Swap:` is the page file (used and total from the system's
    page-file information). A `Commit:` row is not added: scripts parse `free` by `Mem:`
    and `Swap:`.
12. **`nice`** and **`renice`** (coreutils, util-linux): niceness to a priority class:
    `-20…-11` HIGH, `-10…-1` ABOVE_NORMAL, `0` NORMAL, `1…10` BELOW_NORMAL, `11…19` IDLE;
    REALTIME is never set. Bare `nice` prints the niceness that maps back from the
    shell's own class. `renice -n N -p PID…` and `-u USER`; `-g` refused (no process
    groups).
13. **`flock`** (util-linux 2.42.3): `-s`, `-x`, `-n`, `-w`, `-u`, `-o`, `-E`, `-c`,
    `FILE CMD…` and the `FD` form through cash's own descriptor table (`exec 9>lock;
    flock -n 9`), with `LockFileEx` on one byte far past the end of the file, so
    readers of the lock file are not blocked. The lock lives on the shell's handle and
    is released when the file descriptor closes or the command ends. A directory is
    refused: Windows cannot lock one.
14. **`nc`** (OpenBSD netcat's flags, Debian's default): connect and pump standard input
    and output, `-z`, `-v`, `-w`, `-n`, `-u`, `-l`, `-p`, `-k`, `-N`, `-q`, `-4`, `-6`,
    port ranges; `-e` and `-c` refused, as OpenBSD refuses them.

Each tool: a module in `cash-builtins` (Win32 calls in `cash-win32`), an entry in
`builtins.md` under its kind, a help page where its `--help` is not enough, its names in
doctor's `CARRIED` list, unit tests, and an integration test in `crates/cash/tests/it`;
where a Linux original exists it is the oracle, run in WSL (archlinux: util-linux
2.42.3, procps-ng 4.0.7, coreutils 9.11, ncurses 6.6, glibc 2.44, vim's xxd), with cash's
deliberate differences replaced in the test, as `small_tools.rs` does. Help pages and
messages name no spec items. Spec D74 records the decisions; ROADMAP gets item 19.

Decided while building (2026-10-06, by Claude, open to the user): the `free` Swap row,
the `nice` mapping, `flock`'s byte and its refusal of directories, and `nc` without
`-e`, each as written above; the gap analysis asked these as Q5–Q8.

---

## Phase 22. A per-user installer, `cash --update`, and winget

Chosen by the user on 2026-10-06, by pick list (spec D75). The goal: install cash and
have a complete native Bash with the tools scripts need, without Scoop. Scoop stays a
channel; the installer is the second, on the releases page, and winget takes it from
there. The research is in `research/packaging-evaluation.md` ("The installer, for winget
later").

1. **Inno Setup, per-user** (`PrivilegesRequired=lowest`, no UAC), built on the GitHub
   Windows runner where it is preinstalled, attached to each release as
   `cash-vX.Y.Z-setup.exe` beside the zip. Silent for scripts and winget:
   `/VERYSILENT /SUPPRESSMSGBOXES /NORESTART`, `/DIR=` and `/TASKS=`.
2. **Layout as Scoop's**: `%LOCALAPPDATA%\Programs\cash\<version>\cash.exe` and a
   `current` junction, so an upgrade never overwrites a running `cash.exe`; open windows
   keep the old file and new tabs get the new one. Inno copies into the version folder
   and runs `cash --install-finish` (new), which moves the junction, writes the Terminal
   profile, refreshes the tool links, runs `--init-rc --once`, and sweeps old version
   folders nothing runs. `CloseApplications=no`: never Ctrl-C the user's shells.
3. **Tool links on PATH by default**: the install task "Make the tools programs on PATH"
   is on, so `ls.exe`, `sed.exe`, `awk.exe`, … are on the user PATH ahead of Git's;
   a checkbox and `/TASKS=` turn it off. `cash.exe` itself goes on the user PATH always.
   PATH is written by cash (`--link-tools --add-to-path`), never by Inno's `[Registry]`.
4. **Uninstall** (Apps & features entry, per-user): `cash --unlink-tools`,
   `--remove-terminal-profile`, the PATH entries, the folder; the user's `~/.bashrc`,
   history and config stay.
5. **`cash --update`**: asks GitHub's releases API for the latest version, downloads the
   zip, verifies its `.sha256`, unpacks into a new version folder, moves the junction and
   runs the same finish step; `--check` only reports. A cash installed by Scoop says
   `scoop update cash` instead (its exe lives under `scoop\apps`). No background polling
   and nothing automatic: a shell that changes itself overnight surprises script authors.
6. **winget** from the installer (`InstallerType: inno`, `Scope: user`, the silent
   switches above): the first manifest by hand, later versions by `winget-releaser`.
   The installer type cannot change later, so this waits until items 1–5 have shipped
   in a release and been installed on a clean machine.
7. **Unsigned**: a browser download of the setup gets SmartScreen's "unknown publisher"
   once; winget and Scoop downloads do not. Documented in `help installing` and the
   README. Signing stays postponed (the user, 2026-10-06: no paid certificate; SignPath
   not wanted).
8. **An optional Scoop task, off by default** (the user, 2026-10-06): the setup offers
   "Also install Scoop, a package manager for command-line tools" (`/TASKS=scoop`); when
   chosen it runs Scoop's own installer (`irm get.scoop.sh | iex`, in a PowerShell with
   `-ExecutionPolicy Bypass`) as the last step, and says what it did. Installing Scoop by
   default was turned down: it changes the machine beyond what installing a shell
   implies. The installer refuses to run beside a cash that Scoop installed (its exe
   under `scoop\apps`), pointing at `scoop update cash`.
9. **A hint for a missing command, at an interactive prompt only** (the user,
   2026-10-06): after Bash's `jq: command not found`, one line `install it: winget install
   jqlang.jq, or scoop install jq`, from a curated table of about 50 common tools (jq,
   rg, fd, fzf, bat, delta, gh, git, node, python, go, rustup, docker, kubectl, helm,
   terraform, aws, az, make, cmake, ninja, 7z, curl's friends wget and aria2, nano, vim,
   neovim, starship, zoxide, direnv, shellcheck, shfmt, …) with both ids; the table is
   markdown in the help docs (`help tools`), and a test checks each id against
   `winget show` and `scoop info` output recorded once. Scripts and `-c` get Bash's
   message and status 127 only. `cash doctor` names the same two commands for what it
   finds missing.
10. **The zip is the third channel, portable** (the user, 2026-10-06): the release zip's
    `cash.exe` runs from anywhere on its own, and README, the release notes and `help
    installing` say so ("Portable"). A portable cash (one that is neither Scoop's nor the
    installer's) **offers once, at its first interactive prompt**, to put itself on the
    user PATH (so `cash` is a command in any window; the user, 2026-10-06), to add the
    Windows Terminal profile and to put its tools on PATH for other programs; the answer is
    remembered in `%LOCALAPPDATA%\cash\portable-offer`, Enter means no, and nothing on
    the machine changes without a yes. Only at a console, never for `-c`, a script or a
    non-interactive shell; `CASH_NO_OFFER=1` skips it. The alternative, changing nothing
    unless asked, was turned down for friendliness, with the noted risk that a shell
    started interactively by a script sees the question once.
11. **`cash doctor` says when cash itself is not on PATH** (the user, 2026-10-06): its
    `cash` line only reports in-shell resolution, which is always cash, so a portable
    cash whose folder is on no PATH still reads `ok`. Add a `path` line: `ok` when the
    folder of the running `cash.exe` (`current` for an installed one, Scoop's shims for
    Scoop's) is on the user or machine PATH, else `note  path  C:\…\cash.exe is not on
    your PATH: new windows will not find cash` with the fix `cash --add-to-path`. That
    command (and `--remove-from-path`) adds or removes the exe's own folder on the user
    PATH, the same writer the offer and the installer use, and the offer's first
    question calls it.
12. **F1: a one-screen help** (the user, 2026-10-06, by pick list). For Windows users
    who know Bash. The title line is `cash <version>  ·  help`, the version read from the
    running binary, with `F1 or Esc closes` at the right. Layout "Paths, Keys, More" — a line of prose and six path examples
    with a reason beside each (`C:/Users/me/src`; `"C:\Users\me\src"` quoted;
    `'C:\Program Files\Git'`; `/c/Users/me` and `~/src`, which cash reads but other
    programs do not; `tool.exe "$(winpath -w "$d")"`; `$PATH` as the one colon-separated
    variable); seven keys (Alt-E, Alt-←/→, Tab, →, Ctrl-R, Alt-., Ctrl-X Ctrl-E); and one
    block on scripts running unchanged, the built-in tools, `help NAME`, `help topics`,
    `cash doctor`, `start FILE`, `sudo CMD`. Styled in the prompt's colours (section
    names in the accent, keys and examples in the highlighter's yellow and green, notes
    dim; plain under `NO_COLOR`). Drawn on the alternate screen like `less`; F1, Esc or
    `q` brings the prompt back as it was; a short window scrolls with the arrows. F1 is
    a Readline function (`cash-help`) that `bind` can move. The starter `~/.bashrc`
    prints `cash: F1 for help` once per window after the banner. The screen is its
    own, kept to one page; `help` stays the catalogue. Its text lives as markdown
    beside the help topics so it reviews as text and the no-spec-references test covers
    it.

Built on 2026-10-06 (items 1–5, 8 and 9; 6 and 7 follow the first release that carries
the installer): `packaging/inno/cash.iss` and its step in the release workflow;
`cash --install-finish`, `--install-remove` and `--update [--check]`
(`crates/cash/src/installer.rs`, junctions in `cash-win32/src/junction.rs`); the
`installing` and `tools` help topics; the hint in the shell's error formatter; doctor's
`install` line. Two details differ from the plan above: Windows renames a folder whose
exe is running (only the delete fails), so a version folder is judged in use by a
write-open of its `cash.exe` before anything is touched; and `ln -s DIR` makes a real
symbolic link, which needs a privilege, so the `current` junction is set by
`FSCTL_SET_REPARSE_POINT` in place, never half-moved. The installer compiles here (Inno
Setup 6.7.3 from Scoop, 11.7 MB from the 1.5.0 files); an end-to-end silent install,
`--update --check` and uninstall were exercised with HOME, LOCALAPPDATA and the registry
key for the user PATH pointed at a scratch folder, since this machine's cash is Scoop's
and the installer refuses to run beside one. Found there: Inno's uninstaller runs with
Windows' redirection-trust mitigation, inherited by what it starts, so no process of its
may cross the `current` junction ("the path cannot be traversed because it contains an
untrusted mount point"); the uninstaller therefore runs `--install-remove` from a version
folder it finds itself. Windows Terminal, the shell and every ordinary process cross the
junction as before. For item 6, `packaging/winget` holds the three manifest templates and
RELEASING step 8 the procedure; `scripts/sync-bucket.py` copies the Scoop manifest's notes
and hooks into the bucket (step 7). Later the same day: the bucket pushed at 1.5.0 with
the quiet notes (its default branch is `master`; a first push to `main` was taken back),
the first start after the real update measured (see "Found along the way"), items 11
(doctor's `path` line, `--add-to-path`) and 12 (F1) built, and 1.6.0 tagged by the user
on commit 252f6354. The release workflow now installs, runs and uninstalls the setup on
the runner every release (the clean-machine run item 6 asked for). Item 6 done: the
first winget submission, `tomcoolpxl.cash` 1.7.0 from the installer, is
[microsoft/winget-pkgs#447760](https://github.com/microsoft/winget-pkgs/pull/447760)
(2026-10-06, `wingetcreate submit`); once merged, later versions can go through
`winget-releaser` in the release workflow. Left: a look at the F1 screen in the user's
own Terminal profile.

---

## Phase 23. `grep`, `diff` and `cmp`: the standing rule reversed

Chosen by the user on 2026-10-06, by pick list, reversing the 2026-09-25 decision
("cash does not carry grep or diff"). That rule was made when cash lived beside Git
Bash; under the vision of 2026-10-06 (install cash, have everything, no Scoop) `grep` is
the single most common failure on a bare machine and `diff` the second.

1. **`grep`, `egrep`, `fgrep`** with GNU grep 3.x's options, messages and exit codes
   (0 match, 1 none, 2 trouble): `-E -F -G -P`? (`-P` refused, naming `-E`), `-i -v -c
   -l -L -n -h -H -o -q -s -w -x -r -R -a -b -A -B -C -m -e -f --include --exclude
   --exclude-dir --color --null -z`, BRE as the default (`\( \) \{ \} \| \? \+` translated
   to the regex crate's syntax), ERE with `-E`, fixed strings with `-F`. Built on
   ripgrep's library crates `grep-regex`, `grep-searcher`, `grep-printer` and
   `grep-matcher` (BurntSushi, MIT or Unlicense) as dependencies, not copied source:
   ripgrep's engine, GNU grep's interface. A pattern the regex crate cannot take, a
   backreference above all, runs through `fancy-regex` (already a dependency) behind the
   same `Matcher` trait, so the fast path is the common one and nothing is refused. CRLF:
   a line's `\r` is not part of the line (D20), so `grep 'x$'` matches on a CRLF file and
   `-o` never prints a CR; `--binary-files` as GNU. Oracle: GNU grep 3.12 in WSL, case by
   case (`grep_cases`), with `--color` sequences compared too.
2. **`diff`** (unified, context, normal, `-q`, `-s`, `-r`, `-N`, `-i`, `-w`, `-b`, `-B`,
   `--color`, `--strip-trailing-cr`, `-u0`, `-L`) and **`cmp`** (`-s`, `-l`, `-b`, `-i`,
   `-n`) from uutils diffutils (MIT), bundled like `sed`: imported, hardened, exit codes
   0/1/2, oracle-tested against GNU diffutils 3.12 in WSL (`diff_cases`, `cmp_cases`).
   `diff` output is what test scripts compare byte for byte, so the golden file is the
   gate.
3. **The rest follows the decision**: doctor's `EXPECTED` entries for grep and diff
   become `CARRIED`; the `tools` table loses its grep and diff rows; README's "It does not
   carry grep or diff" paragraph goes; `help differences` and spec D48/D35 get the
   reversal; the gap analysis records it; `type -a grep` shows Git's and Microsoft's
   behind the builtin, as for every shadowed tool.

Built on 2026-10-06, for 1.7.0. `grep` reads its patterns through the bundled sed's
translator (`cash_sed::sed::compiler::translate_posix`, made public) with sed's GNU error
wording, so the two tools accept one dialect; what GNU grep reads differently from GNU
sed (a repetition at the start of an ERE, an unmatched `)`, a `{` that opens no
interval) is added around it, and a test holds eleven patterns to the same translation
through both. Backreferences, `\<`/`\>`/`\b` and `-z` run on fancy-regex behind
ripgrep's matcher trait; the rest on grep-regex's prefilters. 714 oracle invocations
against GNU grep 3.12 in `C.UTF-8`; the deliberate differences are the CRLF rule and
name-ordered directory walks. `diff`/`cmp` are a vendored `cash-diffutils` (uutils
0.6.0; CASH-PATCHES.md): GNU's option parser, a change-script engine with `similar`'s
linear-space Myers where upstream's never finished a 100k-line pair, the five printers,
`-r`/`-N`/`-x`, `--strip-trailing-cr`, colour; 102 oracle sections byte for byte against
GNU diffutils 3.12. Doctor's `EXPECTED` list is empty now; the install-hint table lost
its grep and diff rows; README's paragraph says the three are built in. The winget
submission (phase 22 item 6) waits for 1.7.0, so the first winget version carries these
three (the user, 2026-10-06).

---

## Phase 24. `gzip`, `gunzip` and `zcat`

Chosen by the user on 2026-10-06, by pick list (the gap analysis's Q3, reopened): the gzip
family only; `xz` and `bzip2` stay with `tar.exe`. On this tooled machine `gunzip` and
`zcat` resolve to nothing, and `tar.exe` reads archives, not a bare `.gz`.

1. **`gzip`, `gunzip`, `zcat`** with GNU gzip 1.14's options, messages and exit codes
   (0, 1 error, 2 warning): `-c -d -f -k -l -n -N -q -r -S -t -v -1..-9 --fast --best
   --rsyncable`(accepted), `--help`, `--version`; `gunzip` = `gzip -d`, `zcat` = `gzip -dc`;
   files replaced in place with the `.gz` suffix added or removed, times and modes kept
   where Windows has them, the original name and time in the header (`-N`), `-l`'s
   table, `-t`, stdin and stdout, a refusal to write compressed data to a terminal
   without `-f`, `-r` over folders, the `.tgz`/`.taz` suffix rules, `GZIP` environment
   variable accepted with GNU's warning. Pure Rust: `flate2` with `miniz_oxide` (already
   in the dependency tree). CRLF: bytes are bytes. Oracle: GNU gzip 1.14 in WSL
   (`gzip_cases`), byte-for-byte on the compressed output where gzip's output is
   deterministic (`-n`, fixed mtime) and on the decompressed side everywhere.
2. The entries in `builtins.md`, a page, `CARRIED`, the gap analysis's Q3 row.

## Phase 25. Compatibility corners

Chosen by the user on 2026-10-06, by pick list, as the block after gzip; each item is
script-visible Bash behaviour or a kinder refusal.

1. **`/dev/tcp/HOST/PORT` and `/dev/udp/HOST/PORT`** in redirections (`exec 3<>
   /dev/tcp/host/80`, `cat < /dev/tcp/…`, `echo > /dev/tcp/…`), as Bash's: a socket in
   the descriptor table, both directions, Bash's messages on failure
   (`bash: connect: Connection refused`, `bash: /dev/tcp/h/p: Connection refused`).
2. **`command_not_found_handle`** run as Bash runs it: with the command and its
   arguments, its status the command's, the hint only when no function is defined
   (found 2026-10-06).
3. **Persistent `abbr`**: fish keeps abbreviations across sessions; cash keeps them in
   `%APPDATA%\cash\abbreviations` (one `name=expansion` per line), written by `abbr -a`
   and `abbr -e`, read at startup after the rc files, so `abbr -a gco git checkout` once
   is enough; `abbr --no-save`? No: fish has no such flag; a `~/.bashrc` that sets them
   keeps working and wins.
4. **Kinder refusals**: `suspend` (no stop signal on Windows; Bash's words for a login
   shell as the model), `mkfifo` (named pipes are not files on Windows; point at process
   substitution), `stdbuf` (no preload on Windows; `-o0`/`-oL` accepted as a no-op with
   a note when the program is cash's own, refused otherwise) — each a builtin with a
   one-line message and Bash's status, not "command not found".
5. **`getconf`** (`_NPROCESSORS_ONLN`, `_NPROCESSORS_CONF`, `PAGESIZE`, `LONG_BIT`,
   `PATH`, `ARG_MAX`, `NAME_MAX`, `PATH_MAX`, `-a`) and **`locale`** (`-a` listing
   `C`, `POSIX`, `C.UTF-8` and the Windows locales as `en_US.UTF-8` names; bare `locale`
   printing `LANG`, `LC_*`), the values Windows has, GNU's output shapes.

---

## Phase 31. `7z` and `7za`

Chosen by the user on 2026-10-07, by pick list, after 1.9.0: a builtin under both names
(as 7-Zip ships `7z`, `7za` and `7zr`), shadowing Scoop's 7-Zip inside cash, which stays
reachable by its path and `enable -n 7z`; doctor lists it as a deliberate shadow.

The design is `research/archive-tools-design.md` 3.14, set by a second pick list the
same day: 7z and the formats cash has, Windows 7-Zip's words with LF and `/`,
`sevenz-rust2` taken in as cash's own code, volumes, `h`, links, and `-sns`/`-sni` as
7-Zip has them (WIM only, so "Not implemented"). For 1.10.0. Each step ends in a commit.

1. **`cash-archive::sevenz`**: `sevenz-rust2` 0.23.0's source taken in, with its tests;
   the license in `licenses/` and `NOTICE`; brotli, lz4, wasm and the C zstd out; the
   codecs shared; raw block copies; 7-Zip's archive facts and method names; entries in
   order; typed errors, no panics on the two hostile inputs found.
2. **`7z` and `7za` reading** (done 2026-10-07): the parser, `l` (with `-slt`, `-ba`),
   `t`, `x`, `e`, the overwrite question and `-ao`, `-o`, `-p`, `-so`, `-i`/`-x`, `-r`;
   the oracle (`7z_cases.sh`, 7-Zip 26.03 from Scoop). Left for later in the phase: the
   progress line on a console (`-bsp`, `-bd`; done 2026-10-07, 7-Zip's
   `CPercentPrinter` and its hooks in scanning, opening, extracting, adding and `h`,
   matched against `-bsp1` into a pipe), `-si` reading (done with step 4), `-bt`,
   `-spe`, `-snz`, `-ai`/`-ax` beyond names (all done 2026-10-07: archives are found by
   the scan `a` uses, folders entered, `-air`/`-ax` honoured, "Cannot find archive" when
   none; `-bt` reports the command's own times and cycles, the shell process's memory
   peaks), and `i`'s listing is cash's own short one, on purpose: 7-Zip's lists formats
   and codecs cash does not have.
3. **Writing 7z** (done 2026-10-07): `a`, `u`, `d`, `rn`, the `-u` matrix and `-u!name`,
   `-m` (levels, methods, solid limits, filters chosen by a file's head as 7-Zip's
   analysis does, `-mhe`, `-mhc`, times, `-mqs`), `-p` typed or given, `-sdel`, `-stl`,
   `-w`, `-sa`; old blocks copied as they are or repacked; the oracle's update sections
   match 7-Zip 26.03 line for line and its stored archives byte for byte. `-si` came
   with step 4. Left: `-spf` for update, 7-Zip's BCJ2 (it filters x86
   executables with BCJ2 from `-mx8`; cash has no BCJ2 encoder and uses BCJ), analysis
   levels other than the default (`-myx`), memory limits (`-mmemuse` is taken, not
   applied). Compressed archives are not 7-Zip's bytes (other LZMA, PPMd and bzip2
   encoders), so the oracle compares their listings without sizes.
4. **The other formats** (done 2026-10-07): reading gzip, bzip2, xz, zstd, lzma, tar
   and zip; writing tar (GNU and pax headers, long names), zip (7-Zip's records: NTFS
   times, OEM names with the UTF-8 one beside, Store when compressing does not help;
   Deflate, BZip2, LZMA, PPMd, xz; ZipCrypto and AES; kept items copied, renamed ones
   under a new local header), gzip, bzip2 and xz (one file); the format by extension or
   `-t`, an archive there opened as that format only; times compared at the precision
   the format keeps; `-si` for every format, `-so` for tar and the streams; `-si -t`
   reading for `l`, `t`, `x` and `e` as 7-Zip's one-pass opening shows it (tar and the
   streams, "Not implemented" for 7z and zip; standard input kept in a temporary file
   meanwhile, where 7-Zip reads it as it goes); `-mm=` and
   `-mNAME-` as 7-Zip takes them (the 7z path too). `7z_formats.sh` matches line for
   line, its tars and stored zips by `cksum`; 7-Zip tests every archive cash writes.
   Left, where cash differs:
   - zip with Deflate64 says "Not implemented" (no encoder); 7-Zip writes it.
   - xz is one block; 7-Zip on several threads cuts blocks of four dictionaries
     (phase 32). xz's `-mf` filters say "Not implemented".
   - Zip names in code page 437 where 7-Zip uses Windows' OEM code page (850 in
     Belgium); `-mcp` other than 65001 is taken as 437. tar's `-mcp` is not applied.
   - A zip's self-extractor stub is kept but not checked against 7-Zip; Zip64 headers
     follow 7-Zip's one-thread path (its threads decide a file near 4 GiB otherwise).
   Not yet checked against 7-Zip: tar names that are not ASCII (7-Zip prints its UTF-8
   check's status after "UTF8"); zips 7-Zip reads by their local headers (no central
   directory, "Local" in Characteristics), split zips, Unix-made zips' names and modes.
5. **Volumes, `h` and `-scrc`, links, `-sns`/`-sni`**. `h` and `-scrc` done 2026-10-07:
   7-Zip's ten hashes (BLAKE2sp from `blake2s_simd`), its columns and its sums for data
   and for data and names, `-ba`, `-si`; `t` and `x` sum what they read (`t` folders
   too, as 26.03 does). Volumes done 2026-10-07: `-v` writes `NAME.001` and on in every
   format (7z, zip and tar to 7-Zip's bytes), "Volumes: N"; `NAME.001` reads as 7-Zip's
   "Split" level over the archive inside, the volumes not opened again, their sizes in
   the totals. The spanning is cash-archive's `volumes` (`Spanned`, `numbered`,
   `SpannedWriter`), which zip's split parts use too. Links and `-sns`/`-sni` done
   2026-10-07: `-snl` keeps a junction or a symbolic link as one (not entered, its
   reparse data read: a symbolic link in tar, relative to its folder; in 7z and zip the
   reparse attribute, and for a file the data as 7-Zip passes it; `h` hashes the data);
   `-snh` makes a file met again under another name a tar hard link; extraction makes a
   tar's links after every item, from empty placeholders, with 7-Zip's `-snld` danger
   checks and words, `-snl-` leaving symbolic links out; `-sns`/`-sni` "Not implemented"
   when writing, nothing when extracting (no format here has streams). Also fixed: the
   overwrite question now comes before the item's `-` line, a skipped file shows as
   skipped, and a break still reports "Sub items Errors". 7-Zip's links kept in an
   item's data (`is_SymLink_in_Data`) done the same day: a zip's Unix symbolic link
   (mode `S_IFLNK`) and an item with the reparse attribute, under 4 KiB, read whole and
   made a link at the end, or written as they are with "Incorrect reparse stream" (the
   oracle checks a Unix zip's links 7-Zip refuses; a symbolic link actually made needs
   the right this machine lacks, so it is checked only by `cash-win32`'s test, which
   takes either outcome). A made symbolic link does not get the item's times (7-Zip's
   `SetLinkFileTime`). A file answered "No" or skipped: 7-Zip's tar handler
   neither shows nor counts it, its 7z handler shows it at `-bb2` and counts it; cash
   does the latter for every format. `h` on hash files (7-Zip 23's `-thash`) is not
   taken; 7-Zip's split handler also reads `NAME.aa`, `NAME.ab` volumes, cash's only
   numbered ones.
6. The page, `builtins.md`, `CARRIED`, `DELIBERATE_SHADOWS`, doctor (done
   2026-10-07). `CARRIED` has `7z` and `7za`; doctor's new note names a 7-Zip on `PATH`
   outside System32 that cash's `7z` goes ahead of. The phase's full suite passed the
   same day (4374 tests).

---

## Phase 32. Every core, where the format allows it

Asked by the user on 2026-10-07 ("if the algorithm allows it, parallelization is used"),
decided by pick list the same day: after phase 31, released with it as 1.10.0. A survey
the same day found no compressor in cash using more than one thread.

1. **xz** (done 2026-10-07): `XzWriterMt` (lzma-rust2) on all cores by default, as XZ
   Utils 5.8 does (`-T0`), with its block size (three dictionaries, at least 1 MiB) and
   as many threads as a quarter of the memory holds; `-T1` the one-thread stream; `-T N`
   and `-T +1` honoured, `--block-size` still exact. A file of one stream and several
   blocks decodes with `XzReaderMt`; others (lzma-rust2 0.21's threaded reader loses a
   second stream) go through the stream decoder. 64 MB of text: 23.7 s on one thread,
   9.7 s by default (three blocks), XZ Utils 6.9 s.
2. **zstd** (done 2026-10-07): the 4 MiB frames compressed `-T` at once
   (`codec::parallel`), the same bytes for any count; `-T`, `--threads`,
   `--single-thread`, `ZSTD_NBTHREADS` honoured. Every core by default, where zstd
   1.5.7 uses one (the user's choice, 2026-10-07: the bytes do not change); tar's
   `--zstd` too. 64 MB: 0.40 s → 0.12 s.
3. **gzip** (done 2026-10-07): one member of 1 MiB chunks deflated apart on all cores,
   each but the last ended with a sync flush, as pigz lays it out without its shared
   window; input of one chunk keeps today's bytes, and the layout is the same on any
   machine. No new option (GNU's gzip has none). 64 MB: 0.41 s → 0.09 s.
4. **bzip2** (done 2026-10-07): input cut at the block size (`-1` to `-9`, 100 to
   900 kB), each a bzip2 stream compressed on its own core, concatenated (pbzip2's
   layout); one block's input keeps bzip2 1.0.8's bytes. The oracle's `BZIP2=-1` case
   on 300 kB is now three streams (216 bytes, not 134), a divergence in the test.
   64 MB: 2.4 s → 0.39 s.
5. **zip** (done 2026-10-07): members compressed in parallel, written in order, the
   same bytes (`compress_ahead`, `add_ahead`; a test compares with `add`). Batches of
   twice the cores, files up to 64 MiB, 256 MiB in all; encrypted, `-l`-converted and
   larger files as before. 48 files of 1.4 MB: 0.09 s (Info-ZIP 0.86 s).
6. **tar** gets the codecs' threads through `-z`, `-j`, `-J`, `--zstd` (done
   2026-10-07): `codec::writer_on`, each codec's own default (xz as many as memory
   holds, the others every core). `tar -cj` of 64 MB: 12.5 s → 0.56 s.
7. The pages (`gzip.md`, `bzip2.md`, `xz.md`, `zstd.md`, `zip.md`, `tar.md`) say
   what runs on several cores (done 2026-10-07); the oracles pass. The timings above
   were taken by hand on an idle machine; no timing test was added, since one would fail
   on a busy CI machine.
8. Release 1.10.0 with 7z and this: the version raised 2026-10-07, after the full suite
   (4384 tests), clippy with every feature, the doc tests and `cargo doc` passed; the tag
   is yours to push once CI is green.

---

## Phase 33. RAR: `7z` reads it, `rar` and `unrar` builtins

Asked by the user on 2026-10-07 ("can we do rar?", then the link to
[bitplane/rars](https://github.com/bitplane/rars)), decided by two pick lists the same
day: after phase 32, so 1.10.0 ships without it. rars is a pure-Rust RAR library under
Apache-2.0 that reads RAR 1.3 to 7 (encrypted too) and writes RAR 2.9 and 5/7; young
(begun 2026-04, 0.10.0 on 2026-10-03), slower than WinRAR by its own account. 7-Zip
reads RAR and writes none; 7-Zip's own RAR code is under the unRAR restriction and is
not used.

rars is in, as `cash-archive::rar` (its tests in the slow lane). What is left:

1. **`7z l`, `t`, `x`, `e` on `.rar`**, as 7-Zip 26.03's `Rar5Handler.cpp` and
   `RarHandler.cpp` do (their sources in the scratchpad's `7zsrc`, fetched with the
   user's leave on 2026-10-07). `l`, `t`, `x` and `e` are in, RAR 1.5 to 7
   (`sevenzip/rar7`; `7z_rar.sh` and `7z_rar4.sh` check them against 7-Zip on 160 of
   rars' fixtures). Left:
   - 7-Zip's strictness where rar's decoders are WinRAR's: closed 2026-10-10 by the
     user's rule (a faulty archive needs a faulty status, not 7-Zip's words). A RAR 1.5
     volume opened in the middle is a data error in 7-Zip and a CRC error in rar
     (`rar154/random.r00`): both faulty. A RAR 3 VM program that is none of the standard
     filters now fails in 7z as well (`Unpack29::with_standard_filters_only`, as the rar
     builtin has it), a data error where 7-Zip says "Unsupported Method"
     (`7z_rar4.sh`). A match reaching before the start of the data is a data error in
     7-Zip; in 7z too for RAR 5 (finish mode), zeros for RAR 2.0 and 2.9 (`zero_fill/`,
     which WinRAR reads as valid): left so.
   - Copy links (`-oi`): done 2026-10-10. A link points at the last file before it of
     its target's name, version and size (`FillLinks`). One to a file that starts its
     stream decodes that file again; one to a solid file takes a copy kept as that file
     was decoded, up to 4 GiB. 7z now plans RAR from the files asked for, as 7-Zip's
     handler does, and skips a link inside a chain undecoded. `t -scrc` extracts to the
     hasher, and the summary's `Size` counts what a file or the hashes took. 7-Zip's
     own slips are kept: a link decoded inside a solid stream breaks the file after it,
     and a link to a solid file that no stream goes on from comes out empty, "Everything
     is Ok" (`copy_links_*.rar` in `7z_rar.sh`).
   - Output on a data error, RAR 5: done 2026-10-10. The RAR 5 decoder has 7-Zip's
     finish mode (`Unpack50Decoder::set_finish_mode`), found by damaging one byte of
     WinRAR archives at a time and comparing 7z with 7-Zip's verdict, bytes and
     checksum:
     - what decoded before the fault is written;
     - tables must be complete codes or empty;
     - a match before the data, or a repeat never set, gives zeros and fails the file;
     - a main code past a block's end, or a symbol read to the end of its bytes,
       stops; extra bits in the last byte's unused bits, or those bits set, fail the
       file at its end;
     - the data is decoded to its last block, past the size into the window only;
     - a filter of no known type, more than 4 MiB on, or inside the last filter's range
       is skipped as "Unsupported Method" (a data error wins over it, it over CRC);
     - a filter past the file's end is kept, and nothing from its start is written;
     - in a solid stream the window and tables carry on after a fault, the next file
       starting at the size's end at the soonest.

     Every offset of the sweeps matches (text, an executable with E8 filters, Delta,
     ARM, solid; `7z_rar.sh` keeps a handful). The rar builtin keeps the old decoding:
     WinRAR's own on damaged data is not observed.
   - Output on a data error, RAR 1.5 to 4: closed 2026-10-10 by the user's rule that a
     faulty archive needs a faulty status, not 7-Zip's exact bytes. Both give one;
     7-Zip's old decoders flush their window in large chunks, so a small damaged file
     gets nothing written in either, and the error word differs on some offsets (CRC
     against data error, both ways; 5 of 30 offsets of `rarvm/filter_bsdcat_exe.rar`
     and `rar250/unpack20_keep_tables.rar`).
   - Alternate streams (`STM`) and ACLs: done 2026-10-10. A stream is listed with
     `-sns` only, its count and sizes on lines of their own in `l`'s totals and in
     `t`/`x`'s summary ("Alternate Streams"), wanted with its file, and extracted into
     its file's stream unless `-sns-` (cash wrote `a.txt_note`). An ACL is decoded at
     open, its file's "ACL" characteristic, and its `NT Security` is 7-Zip's own text
     for a descriptor (owner, group, `s:`/`d:` entry counts, size; not SDDL as this
     line said), checked on 48 made-up descriptors (`nt_security_crafted.rar`). Left:
     `x -sni`, which 7-Zip uses to set the ACLs on what it extracts.
   - An SFX's archive inside its `.exe`: done 2026-10-10. A file that opens as nothing
     at its start (a tar whose first checksum is wrong does not), and is not named as
     RAR, is searched for a RAR 5 signature starting at most 8 MiB in (7-Zip's limit,
     to the byte), opened there, and listed with `Offset`; a `.exe`, `.dll` or `.sys`
     that is no PE adds 7-Zip's "Cannot open the file as [PE] archive" warning. A RAR
     name is tried as RAR only, as 7-Zip does (no tar or lzma guess after it). Left: a
     RAR 1.5 to 4 SFX (WinRAR 7.23 writes none to observe), and other formats after an
     SFX module (7z's own `.sfx`, zip's).
2. **`rar` and `unrar` builtins** with RARLAB's console interface, reading and writing:
   designed in `research/archive-tools-design.md` 3.15 and picked by the user on
   2026-10-07/08 (the fuller set, RAR 5, Scoop's WinRAR 7.23 as the oracle, built by
   observation only). Done: the command line (`RARINISWITCHES`, `rar.ini`, every switch
   read), the banner and `-?`, and the listings `l`/`lt`/`lta`/`lb`/`v…` with `-v`
   (`rar_list.sh`, 12,470 lines like WinRAR's); `t`, `x`, `e`, `p` with the overwrite
   question and `-o`, passwords asked per file, the path switches, volumes from any
   one, damaged headers (rars' lenient read), recovery records and `.rev` files
   (`rar_extract.sh`, 2,880 lines); `a`, `u`, `f`, `m`, `mf`, `d` with the archiving
   switches, RAR 5 only (`-ma4` refused, the user's pick, 2026-10-08), stored archives
   and volume sets WinRAR's byte for byte (`rar_write.sh`, 1,634 lines). For that rars
   gained a WinRAR layout (`rar50::Layout`: padded sizes and offsets, the locator, skip
   flags, CRC32 or BLAKE2, the quick-open threshold, volumes counting their headers) and
   carried members (`Builder::carry`: an archive's packed data copied on update, RAR 1.5
   to 7, as rar copies it). Left, in order:
   - `t` of a RAR 3 `.rev` file: done 2026-10-09. Damaging them under Rar.exe showed
     the newer naming's last four bytes are a CRC32 of the rest (and the three before
     them its counts and number), which `t` checks; the older naming's are not tested
     by rar, nor by cash (`rar_reconstruct.sh`).
   - Extraction: `-f`/`-u`, `-ad1`/`-ad2`, folders' times and attributes (`-ai`
     keeping the times only), and one summary for the archives a wildcard matches,
     their errors added up: done 2026-10-09 (`rar_extract_switches.sh`, 363 lines; this
     line said "built" of them, but extraction read none, set no folder's time, and
     summed up each archive alone). `-ts`, `-tsc`, `-tsa` and `-tsm-` on extraction:
     compared with Rar.exe by hand, not in the oracle.
   - Links on extraction: done 2026-10-09 (`rar_links.sh`, 176 lines; extraction wrote
     each link as an empty file, saying OK). Hard links, file references, `-ol-`, and
     symbolic links skipped as unsafe (absolute, or more `..` than the link's own
     folders) are in the oracle, with two archives of cash's own fixtures
     (`crates/cash/tests/fixtures/rar`: `copies.rar` made by Rar.exe `-oi`,
     `links_unsafe.rar` rars' `symlink.rar` with its targets changed). Left out of it,
     compared by hand: a symbolic link Windows refuses for want of the right ("Cannot
     create symbolic link ... You may need to run RAR as administrator", a folder's
     link leaving an empty folder, as rar's does) and one it makes, which needs that
     right.
   - A file reference whose file is not asked for: done 2026-10-09 (in
     `rar_links.sh`, now 319 lines). The file is unpacked first to a temporary
     `__tmp_reference_source_N.N.rartemp` in the destination, as typed (`x//` for `x/`),
     the reference copied from it, the temporary file removed at the end; in a solid
     archive too, where cash had nothing left to copy.
   - A solid archive's files after one not asked for: done 2026-10-09
     (`rar_extract_switches.sh`, now 449 lines). cash decoded a skipped file only when
     it continued the stream, so a solid archive's first file, when not asked for, was
     not decoded and every file after it failed its checksum: `rar x solid.rar
     dir\later` gave "checksum error". Now each file not asked for is decoded on the
     way, a "Skipping" line said for it in `x`, `e` and `t` (none in `p`), and nothing
     is read after the last file asked for; with none asked for, every file is skipped,
     as rar does.
   - Junctions on extraction: done 2026-10-09 (in `rar_links.sh`, now 226 lines).
     cash made a junction a plain folder, whatever the switches. Now a junction is a
     link: skipped as unsafe (its target is absolute) without `-ola`, left out with
     `-ol-` ("No files to extract", rc 10), and made a junction with `-ola`, its line
     ending in rar's "   ? " after the line of the folder made for it. A link's folder
     is made only once the link is to be made, after its line, as rar does.
   - `-ol` on `a`: done 2026-10-09 (in `rar_add_links.sh`, now 613 lines). A junction
     is followed without it (its target's files under its name, its own entry a plain
     folder with its own time), left out with `-ol-`, and with `-ol` or `-ola` stored as
     a type-3 link: a folder entry with the reparse attribute (0x410), the substitute
     name its reparse data holds (`\??\C:\...`) with `/`, flag 1, taken by a mask
     without `-r` as a file is. A tree holding a junction used to fail whole ("Access
     is denied"). `lt` named a junction "Windows junction", where rar says "NTFS
     junction point", and gave a folder link sizes, which rar leaves out. Symbolic
     links (type 2, the substitute name likewise) are not observed: making one needs a
     right this desktop lacks; rar's word for them, "Windows symbolic link", was seen by
     listing a junction archive retyped to 2.
   - `-oh`, hard links: done 2026-10-09 (in `rar_add_links.sh`, now 422 lines). A
     file's other names put in after its first are type-4 links to it, their sizes not
     padded, with the times the folder's entry gives (a name not used since the file
     changed keeps old ones there, and rar stores those). `-oi` counts them among the
     identical files, and they stay hard links. A file's own size and times are now
     taken from the file opened, not its folder's entry, as rar's are.
   - `-oi`, identical files as references: done 2026-10-09 (`rar_add_links.sh`, 328
     lines, the archives WinRAR's byte for byte). Files of the least size or more
     (64 KB, or `-oi:SIZE` with rar's units) with the same size and the same bytes,
     among those put in: each set's first in archive order is stored, the rest as
     type-5 references to it, their sizes padded as a file's. "Searching for identical
     files" and the count, `-oi2` listing the sets first (by size, smallest first, a
     blank line between), `-oi3` and `-oi4` listing only, no archive and for `-oi4` no
     banner. rars' builder has `add_link` for any kind now. `-oi` with volumes: done
     2026-10-10 (in `rar_add_links.sh`, now 676 lines, each volume WinRAR's byte for
     byte): rars' WinRAR volume framing now carries a link's record (a hard link's
     sizes unpadded), where cash refused ("symbolic links are not supported in volume
     output"). A solid archive's reordering with `-oi` was compared by hand, its
     listing Rar.exe's. A solid set's later volumes say "Creating solid archive" (cash
     said "Creating archive"); where a compressed set splits stays cash's own.
   - `-ver`: done 2026-10-09 (`rar_versions.sh`, 315 lines, the archives WinRAR's byte
     for byte): rars reads and writes the version record, `a`/`u`/`f` keep the file
     replaced as `name;N` with `-ver`, `-verN` drops the oldest beyond N and numbers
     the rest again, listings show `;N` and lt "File version", `x`/`t`/`p` take an older
     version only named exactly, all with `-ver`, version N plainly named with `-verN`,
     `d` matches the `;N` name. Versions in a solid archive: done 2026-10-09 (in
     `rar_versions.sh`, now 330 lines): its members, written again rather than carried,
     lost their numbers, and a second `-ver` update failed on "duplicate archive entry
     name"; rars' builder's `set_version` now numbers any member, carried or not. Left:
     a file comment on a member with older versions (set by name, it may land on the
     wrong one).
   - A solid archive's update: done 2026-10-09 (`rar_solid.sh`, 458 lines, the
     counters' backspaces and shares kept). Rar.exe says "Updating solid archive" and
     shows its repacking as it goes through the members: with a file replaced or a
     member dropped, every member kept is repacked, counted run by run after
     "Repacking archived files:" (seven columns, backspaced over, and the share of the
     bytes written so far after each holding data, not in `d`); a member replaced or
     dropped is said, and before the next kept one "Analyzing archived files:" counts
     those since the last; a file added leaves the run going, its next count written
     after the file's OK. With files only added, the old members before the first are
     analyzed, those after repacked. `d` with nothing to delete repacks every member
     before "No files to delete". `-idp` shows none of it. A file's line under `-idp`
     keeps four spaces where its share would be (cash kept none: no oracle had `-idp`
     on `a`).
   - A damaged header that still parses: done 2026-10-09 (in
     `rar_extract_switches.sh`, now 568 lines). rars' lenient read stopped at the first
     header failing its checksum; past the main header it now keeps it, marked
     `damaged`, as Rar.exe does: `t`, `x`, `l` and `lb` say "Corrupt header is found"
     and "NAME - the file header is corrupt" on standard error, two errors each, rc 3,
     and use the header as it stands. A time past 2185 is read wrapped around, as rar
     keeps times as nanoseconds in 64 bits (a damaged one showed as 1899). Its hour
     before 1914 is the zone's own of the day in cash, Windows' present one in Rar.exe
     (masked in the oracle; a 1795 date was 42 minutes apart).
   - `-ts` on `a`: done 2026-10-09 (`rar_times.sh`, 73 lines, twelve kinds and
     precisions, the archives WinRAR's byte for byte; PowerShell sets the times first).
     One precision a file: whole seconds only when every time kept asks for them, kept
     then as 4-byte Unix seconds (rars' `FileTimestamp::UnixSeconds`, so such a record
     also reads and is carried as it was); else all at full precision. lt now shows
     "Created" and "Accessed". RAR 1.5 to 4's creation and access times: done
     2026-10-09 (in `rar_times.sh`, now 132 lines). WinRAR 7.23 writes no RAR 4, so
     rars wrote one with them in its extended-time field (cash's fixture
     `times_rar4.rar`): Rar.exe lists them as "Created" and "Accessed" and restores
     them with `-tsc`, `-tsa` and `-ts`, local DOS times to the 100 ns; rars decoded
     only the modification time. rars' RAR 1.5 to 4 header has `ctime` and `atime`.
   - `-df`, `m` and `mf` with a folder left holding files left out ("NOT DELETED",
     "Cannot delete ... The directory is not empty."), and `-r` with a plain file name,
     looked for in every folder below its own as a mask is (not with `-r0`, and a name
     found nowhere is then no error): done 2026-10-09 (`rar_add_names.sh`, 152 lines;
     the first was already rar's, the second is new).
   - The locator's padding when an archive is changed: done 2026-10-09 (in
     `rar_add_names.sh`, now 326 lines). WinRAR's bound counts the old archive's
     members, dropped ones too, and for `a`, `u` and `f` every file named, written or
     not; cash counted the members written, so an update or `d` near a width's edge
     came out a byte short (rars' `Layout::offset_bound`, given by cash). An old member
     counts by its packed size (done 2026-10-09, in `rar_add_names.sh`, now 391 lines:
     seen with compressed members near a width's edge); and a compressed member copied
     has its data size written as wide as its unpacked size takes, a stored one's not
     padded, where cash wrote both unpadded.
   - A file another program has open for writing: done 2026-10-09 (in
     `rar_add_names.sh`, now 192 lines). Rar.exe cannot open it, as it opens what it
     adds letting no one write ("Cannot open NAME / The process cannot access the
     file because it is being used by another process", then "WARNING: Cannot open N
     files", rc 6), unless `-dh`; cash read it, and its writer stopped on "entry source
     size changed while reading". Names not there now give rc 10, as rar's do, not 1.
   - Volume sets with `-rr`, `-p` and `-hp`: done 2026-10-09 (`rar_volumes.sh`, 130
     lines). They kept rars' own payload-sized layout, so a set with a recovery record
     or encrypted headers came out larger than the size asked; now WinRAR's: each
     volume's recovery record after its quick-open one, covering what is before it and
     sized from the room left, and `-hp` headers encrypted in every volume, no
     quick-open record. The quick-open room kept for a file going in is its header and
     18 bytes, for each earlier one in the volume its header and 16 (seen in twelve
     `-rr` sets and a `-p` one, all WinRAR's to the byte). A volume's number has the
     digits of the volumes foreseen from the bound, as Rar.exe's. A folder among
     encrypted files has no encryption record, in volumes and single archives alike, as
     WinRAR writes it (it was 49 bytes larger).
   - `-hp`'s quick-open block: done 2026-10-09 (`rar_volumes.sh`, now 162 lines;
     `rar_modify.sh`). WinRAR keeps one under encrypted headers, in one archive and in
     each volume: the cached headers as they lie encrypted, encrypted again with the
     headers' key under an encryption record of its own (the archive's salt, an IV, a
     password check of eight zero bytes and their SHA-256's first four), padded to 16
     bytes, the padded length its size. In a volume it is kept 16 bytes more room, and
     an encrypted recovery header 54 (it takes 48). The locator's offsets count from
     the main header, after the encryption header: cash's counted from the signature,
     38 bytes off under `-hp`. A split part's encryption record does not say its
     checksums are keyed to the password (only the last part's does, and none under
     `-hp`): WinRAR's `t` called every part of cash's `-p` sets "packed data checksum
     error". cash refused to change an archive with an encrypted quick-open block, so
     WinRAR's `-hp` archives could not be updated; it changes them now, its own too.
   - A change drops the recovery record unless `-rr` asks again: done 2026-10-09
     (`rar_modify.sh`, now 607 lines). `a`, `u`, `m`, `d`, `c`, `rn`, `k` and `ch` all
     write the archive anew without one, as WinRAR does; cash kept it. `-rr` on `d`,
     `k` and `c` adds one, its line before "Locking archive" for `k`. `ch -tl` alone
     leaves the archive's bytes alone and sets its time only; `ch -k` says "Locking
     archive". The locator's padding when changing counts the old archive's recovery
     and quick-open records as members too (a byte short before).
   - Writing, not yet in the oracle or not yet WinRAR's: (updating
     an archive whose files are encrypted without its password done 2026-10-09, in
     `rar_add_names.sh`: a non-solid RAR 5 archive's members are carried by rars'
     new `carrying_builder`, which asks the password only for encrypted headers and
     comments; a solid one's, read back and written again, still need it); (`-log` done 2026-10-09, `rar_log.sh`: A, F, P, U
     for a, x, t, l, lb and d, in UTF-8 where Rar.exe writes ANSI unless -sc...g says; `-ag` done 2026-10-09,
     `rar_agname.sh` and unit tests from the names Rar.exe gave; `-as` done
     2026-10-09: what no name gives taken out, its "Deleting" lines in their places; and
     `-dw` and
     `-dr` done 2026-10-09: `-dw` in `rar_add_names.sh`; `-dr`, a folder archived whole
     recycled whole and lines without a share, compared with Rar.exe by hand, out of
     the oracle not to fill the Recycle Bin);
     `-md` done 2026-10-09 (in `rar_add_names.sh`): the dictionary asked
     (32 MB without), halved while the largest file, or all in a solid archive, fits
     twice, to 128 KB (1 MB solid), as the headers Rar.exe writes record it; rars' builder's
     `rar50_dictionary_size` is public now. Sizes above 4 GB (RAR 7's) are not tried.
   - `c`, `cw`, `rn`, `k`, `rr` and `ch`: done 2026-10-09 (`rar_modify.sh`, 474 lines then,
     the rewrites WinRAR's byte for byte). Left of them: `ch` takes `-cl`, `-cu`, `-z`,
     `-k`, `-tl`, `-rr`, `-ts`, `-ams`, `-amr`, `-qo` (its three forms, seen WinRAR's
     byte for byte on 2026-10-09) and `-hp` (below). `-ep`, `-htb`, `-s`, `-p` and
     `-df` change nothing there; `-ma4` is refused as by cash.
   - `-hp` on the commands that change an archive: done 2026-10-09 (in
     `rar_passwords.sh`, now 791 lines, by size and details, the salts being
     random). `a`, `u`, `f`, `m`, `d`, `c`, `k`, `rn`, `ch` and `rr` encrypt a plain
     archive's headers with it; cash did in `a` alone. An archive whose headers are
     plain and some files encrypted is refused: "Cannot change the header encryption
     mode in already encrypted archive", rc 2, after `a`'s "Updating archive" and
     `c`'s comment read, before `k`'s "Locking archive". A comment written under
     encrypted headers is encrypted as a file is, with the headers' salt and key, its
     own IV and the password's check, zero-padded to 16, the padded text's CRC its
     own; cash stored it plain inside them (60 bytes short). A comment carried keeps
     its form (rars' builder's `comment_kept_plain`). An encrypted file is never
     stored for want of compression: Rar.exe packs seven bytes into 32 with `-m3`
     where cash stored them (rars' compression plan's `keep_method`). With `-idq`,
     "Program aborted" is hidden, as a message; a run that stops leaves standard
     error's line as it is, and one that ends ends standard error with a line's end
     whenever it wrote there, an ended line getting another.
   - Passwords typed, and archives that do not open to be changed: done 2026-10-09
     (`rar_passwords.sh`, 676 lines then). A bare `-p` or `-hp` is asked for as rar reads
     it, before its banner and the other switches' words, unless a password came
     before it (`-hp` takes `-p`'s; `-p` asks after `-hp`'s); cash asked after its
     banner. A password from a pipe is all one read gives, its line ends trimmed
     (`x\ny` is wrong). After a password typed for the headers or a file, no line
     ends. A wrong one typed is "The specified password is incorrect." and asked for
     again until it is right or the input ends; cash said "Incorrect password". After
     a password typed for a file, each encrypted file asks "NAME - use current
     password? [Y]es, [N]o, [A]ll" (No asks for another, All stops asking and makes a
     wrong one an error). The end of the input at a question for a file's or the
     headers' password says "The pipe has been ended." for a pipe. `-p-` asks nothing
     and finds encrypted headers' password wrong. `c`, `k`, `rn`, `ch` and `rr` say
     "Processing archive" before the headers' question, and of a wrong password
     "Incorrect password for X / ERROR: Bad archive X", rc 11; `a`, `u`, `f`, `m`, `d`
     and `cw` stop with "Incorrect password for X" and "Program aborted", rc 13 (Bad
     archive, in Rar.txt). A file that is no RAR archive: "ERROR: Bad archive X" with
     "Program aborted", rc 13, for `a`, `u`, `f`, `m`, `d` and `cw`; with "Processing
     archive" and Done, rc 0, for the others that change; "No files to extract", rc
     10, after `x`, `e`, `t` and `p`'s "is not RAR archive"; nothing from `lb`. `r`
     on these: done 2026-10-09 (in `rar_repair.sh`, now 373 lines). It asks no
     password: headers it cannot read, encrypted or no archive's, are searched for a
     recovery record by its chunks' marks (rars' `recovery_by_marks`), which mends
     them; without one, encrypted headers give "No files found", rc 10, and a file
     that is no archive a second search, "Reconstructing", "Building", corrupt-header
     and "No files found" words, rc 10, and nothing written. Encrypted headers are
     never rebuilt, read or not. The order of switches: done 2026-10-10 (in
     `rar_passwords.sh`): rar asks at a bare `-p` before reading an `-inul`, `-idq`
     or `-ierr` after it, so the question and "Program aborted" heed only those
     before; cash's heeded all.
   - `-ams`, `-tl` and `ch -amr`: done 2026-10-09 (in `rar_modify.sh`, now 981 lines,
     WinRAR's byte for byte but a recovery record's bytes). `-ams` saves the archive's
     file name and the time it is written (with `-tl` its newest file's, folders left
     out) in its main header, for `a`, `u`, `f`, `m`, `d`, `c`, `k`, `rn`, `ch` and `rr`;
     cash saved neither. `-tl` sets the archive's own time to its newest file's after
     every one of them; cash did it for `ch` only. `ch -amr` sets the archive's
     creation and modification times to the saved one, then renames it to the saved
     name ("X is renamed to Y"), asking "Overwrite NAME?" with Yes, No, All, nEver and
     Quit when the name is taken (`-o+` and `-y` replace, `-o-` leaves, `-or` fails as
     Rar.exe's does: "Cannot rename X to NAME(1).rar", rc 0); the file replaced is
     deleted first, so the archive takes its creation time as Windows gives it; other
     switches are ignored. Rar.exe deletes the archive itself when the saved name
     differs only in case; cash renames it (in "Found along the way"). `lt`'s
     "Original time" now reads a saved time stored as Unix seconds or nanoseconds too.
     `rn` of two files to one name kept the first only and said "duplicate archive
     entry name"; rar keeps both under the name, and so does cash (rars' builder's
     `rename_by_id` honours `allow_duplicate_names`).
   - A change stores the kept files' times again by `-ts`: done 2026-10-09 (in
     `rar_times.sh`, now 118 lines, WinRAR's byte for byte). `a`, `u`, `f`, `m`, `d`,
     `c`, `k`, `ch` and `rr` write every member kept with the times the switches keep,
     at their precision: by default the modification time alone, its creation and
     access times dropped; `rn` keeps them as they are. cash carried them as they were.
     rars' builder gains `last_entry_id` and `set_mtime_by_id`.
   - `c`, `k`, `rn`, `ch` and `rr` on a volume: done 2026-10-09 (in `rar_modify.sh`, now
     757 lines, WinRAR's byte for byte but a recovery record's bytes). cash refused a
     volume ("Cannot modify volume"); rar changes the volume named by itself, whichever
     it is, the others left as they are (`rn` renames that volume's part only), and
     writes it without its zero fill; `d` it refuses still. rars' builder writes one
     volume of a set (`volume_of`: its number, and more to follow in its end header),
     carrying split parts, and rewrite's `volume_builder` sets it up; a volume's zero
     fill and "more volumes" end flag no longer count as unsupported. `t` now tests a
     volume's recovery record as it leaves that volume, where it tested them all at the
     end.
   - Recovery records above 100%: done 2026-10-09 (in `rar_modify.sh`, now 639 lines).
     cash took anything above 100 as the default 3%; now up to 1000%, more recovery
     shards than data ones, as WinRAR writes them; `-rr0` none; above 1000 rar's
     "Adjusting -rrN value to 1000." on standard error and, as Rar.exe was seen to,
     200%'s record. A service's sizes take room for twice them and a kilobyte, without
     a file's five bytes from a mebibyte on (a 1.2 MB record took four). Reading,
     testing, repairing and changing such an archive work. With `-idq`, rar ends
     standard error's open line at the end ("Cannot open ..." too); cash does now.
   - `i`: done 2026-10-09 (`rar_find.sh`, 264 lines then). Its tables and a solid
     archive's names: done 2026-10-09 (`rar_find.sh`, now 369 lines). `t` looks in the
     OEM code page too, whose control codes show as the console's pictures (`◙` for a
     line feed); ANSI is Windows' code page (1252: 0x82 a low quotation mark), where
     cash read Latin-1; letters fold in every table (`CAFÉ` finds UTF-8's `café`). A
     file named in a solid archive: each file decoded on the way writes over a
     progress's place, five backspaces and five spaces, as Rar.exe does.
     `cash_win32::codepage::decode_glyphs` decodes with the console's pictures.
   - `r`: done 2026-10-09 (`rar_repair.sh`, 253 lines; `t` now tests a recovery record
     by its own chunks' checksums, and finds an archive "ended early" only where its
     locator points at a missing quick-open record, as rar does). Damage the record
     cannot mend: done 2026-10-09 (in `rar_repair.sh`): each damaged block "cannot
     recover data", "0 blocks are recovered", and the archive rebuilt instead; cash
     said nothing of them and wrote nothing. A recovery record whose header fails its
     checksum: done 2026-10-09 (in `rar_repair.sh`). Rar.exe reads on past a service
     header as past a file's, "RR - the file header is corrupt" in `l`, `t` and `x`
     (two errors), the record still tested by its chunks; rars' lenient read now keeps
     it, where it ended the walk ("Unexpected end of archive"). `r` mends with it and
     leaves it out of `fixed.NAME`, which ends where it began with an end header, as
     Rar.exe's does byte for byte. A damaged main header: done 2026-10-09 (in
     `rar_repair.sh`, now 494 lines): a record looked for by its marks, then
     "Main archive header is corrupt" twice and "The archive header is corrupt. Mark
     archive as solid? [Y]es, [N]o" (Yes alone says solid; `-y` does not answer; the
     input's end aborts, rc 12), the new main header `03 01 04` with the solid flag or
     none, the rebuilt archive Rar.exe's byte for byte; cash wrote header flags 0 and
     asked nothing. Left of it: RAR 1.5 to 4 rebuilds, unobserved (WinRAR 7.23 writes
     no RAR 4 to damage).
   - `rc`: done 2026-10-09 (`rar_reconstruct.sh`, 249 lines, RAR 5 and both RAR 3
     namings, the volumes rebuilt WinRAR's byte for byte); `s` and `rv` refused in
     words of their own. Left of it: a RAR 3 volume that is there but damaged (its
     recovery volumes keep no checksum of it; what rar does then is unobserved), and
     old-style volume names (`NAME.rar`, `NAME.r00`: WinRAR 7.23 names RAR 5 volumes
     `.partN` even with `-vn`).
   - The `expect(dead_code)` at the top of `rar/mod.rs`: gone 2026-10-09, with the
     items it covered that nothing used; extraction now restores the creation and
     access times `-ts`, `-tsc` and `-tsa` ask for, and `-tsm-` leaves the modification
     time as written, as Rar.txt says.
3. **The page, `builtins.md`, doctor** (done 2026-10-09): `help rar` covers both names;
   `CARRIED` and doctor's note on a program cash goes ahead of have `rar` and `unrar`,
   so a WinRAR on `PATH` is named with the way to reach it; 7z's page already says it
   reads RAR.
4. **rar's spool files outlive a killed process**: done 2026-10-08. Spools and reader
   scratch files are opened delete-on-close, a spool keeping that handle while it lives,
   and a writer with no temp folder named spools in the system's, not the process's
   working folder. `rar a` passes `-w`'s folder when given.
5. **rar's AES key schedules are not wiped**: done 2026-10-10, the user's pick (a
   download of `zeroize`, in the end found in cargo's cache). The `aes` crate's
   `zeroize` feature is on, so `Aes128`/`Aes256` wipe their round keys when dropped,
   for zip and 7z as well as RAR; the passwords, keys and IVs stay `crypto::wipe`'s.

---

## Phase 34. The prompt, paths, and a guard on size and speed

Asked by the user on 2026-10-10, after 1.11.0, on what they like most: small and fast,
every way of writing a path working, and the prompt's helpers. Design in
`research/prompt-history-and-jump-design.md`; the choices made by pick list the same
day (spec D79, D80). In order:

1. **Ctrl-R: a history picker** (D79, done 2026-10-10): Alt-E's inline list over the
   history, fuzzy, newest first, Enter to the line and Tab to run, Ctrl-R again for what
   ran in this folder (a folder record of cash's own beside the history file,
   `%LOCALAPPDATA%\cash\history-folders`; `CASH_NO_RECORDS` keeps none, and the tests set
   it). Keys typed straight after Ctrl-R or Alt-E now reach the picker, not the line
   after it (reedline patch 10). The folder list shows only commands still in the
   history, so `history -c` empties it too. `reverse-search-history` is Reedline's search
   again, which `bind` can put back on Ctrl-R. Help: `keys`, `history`, `vars`,
   `differences`, F1, README.
2. **`z`: the folder jump** (D80): every change of folder ranked across sessions as
   zoxide ranks, `z WORDS`, `z -l`, `z -i`, Tab, and Alt-E's Alt-H on the same record.
3. **Every way of writing a path, everywhere**: each spelling in each place a path is
   typed or written, as a table of tests; what fails fixed.
4. **A guard on size and speed**: `cargo xtask perf`, `perf-budget.toml`, run by the
   release workflow before it publishes.
5. **`rm -rf` on junctions**, from "Found along the way": a junction inside a folder
   gives "Permission denied", and one whose target went first fails too; both need the
   link removed as a folder link, never followed (`uu_rm` 0.12.0).

---

## Found along the way

- **cash's `printf` writes a line's end in a write of its own** (found on 2026-10-09
  with `rar_passwords.sh`): done 2026-10-10. `printf 'v\n'` into a pipe whose reader
  was waiting gave it `v`, then `\n`, and Rar.exe took the empty line for a cancelled
  password. Each pass of the format is now held and written at once, as bash's printf
  writes it (a Python reader sees `v\n` whole; unit test in `printf.rs`).

- **WinRAR matches an archived name typed with `/` to nothing** (found on 2026-10-09
  with `rar d`): Rar.exe 7.23 on Windows takes `\` alone between folders in the names
  after the archive's, so `t`, `p`, `e`, `x` and `d` given `src/a.txt`, `*/a.txt` or
  `src/*` say no files and return 10, where `src\a.txt` works. cash's rar takes both,
  which in a shell where `/` is typed is what a user means. Kept so, the user's pick
  on 2026-10-10; the oracles use `\`.
- **`ch -amr` on a saved name differing only in case deletes the archive in Rar.exe**
  (found on 2026-10-09): `M2.RAR` saved as `m2.rar` is asked about as taken (it is
  itself), and on Yes or `-o+` Rar.exe deletes it, then cannot rename it ("Cannot
  rename M2.RAR to m2.rar / The system cannot find the file specified.", rc 0). cash
  renames it instead, a difference kept on purpose and out of the oracle.
- **Rar.exe's recovery record of a tiny archive differs between runs** (found on
  2026-10-09): `rr` on the same 156-byte archive twice gave chunks with different
  checksums and parity, the data shards' checksums alike; what it pads a short shard
  with seems not to be zeros. Larger archives' records are the same each run, and
  cash's match them; the oracles show a small archive's record by its size.

- **The ConPTY tests that run PowerShell time out in CI now and then**: PowerShell
  started by the test prints nothing in 30 s (`conpty_interactive_tests.rs:220`).
  Run 37953062174 (2026-10-09, slow lane 1/4, both tries):
  `conpty_a_program_that_ran_leaves_a_healthy_prompt_where_it_was`, on a runner whose
  setup also logged a "bash startup failure". Run 37963172593 (the same day, slow lane
  4/4, both tries): `..._leaves_a_scroll_region_...`, `..._dies_in_full_screen_...` and
  `..._had_vt_input_on_does_not_eat_keys_typed_ahead` (line 402). Runs between passed;
  the commits touched only rar. Look at PowerShell's start on the runner (a first-run
  cache?) before raising the wait.

- **`rm -rf` stops at a junction** (found on 2026-10-09 with `rar_add_links.sh`): a
  folder holding a junction gives "rm: cannot remove 'top\junc': Permission denied",
  rc 1, in 1.10.0 too. It does not follow the junction (the target's files survive); it
  fails to take away the link itself, which a folder link needs `RemoveDirectoryW` for.
  `rm` is uutils' `uu_rm` 0.12.0 from crates.io: the fix means a near-upstream copy in
  `vendor/uutils` (as `uu_sort` and the others), or seeing whether a newer uutils has it.
  The oracle takes its junction away with `cmd /c rmdir` meanwhile.
- **`seven_z_matches_7_zip_26_03` failed twice** in nextest runs of the twelve 7z tests
  on 2026-10-07, each right after a build, and passed alone and in seven runs after,
  three of them beside the other three oracle scripts; the difference was not caught.
  Run it with `--failure-output final` until it fails, and see what moved.
- **7z asks for a password before extracting, 7-Zip when it gets to the first
  encrypted item** (found on 2026-10-07 with RAR, true of zip too): with no password to
  type, 7-Zip has written the items before it and leaves that item's empty file. In the
  page's notes; asking at the item would mean handing the password to a decoder already
  under way.
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

- **Archive tools: what is left out on purpose** (phases 29 and 30, 2026-10-07; zip's
  repair, splits and line ends, unzip's other methods, AES and wildcard archives added
  the same day). zip refuses `-R`, `-U`, `-A`, `-J`, `-DF`, `-sp`, `-AC`/`-AS` and the
  logs; unzip skips tokenized, PKWARE DCL, Terse, LZ77 and WavPack members. zip's `-sv`
  names the parts after writing, not as Info-ZIP does while it writes. Each can be added
  when someone needs it.
- **Three zip choices made without a pick list** (phase 30, 2026-10-07, under "do not
  stop"): deflate on `miniz_oxide`, the RID as the numeric owner, and "made on Unix" (design
  3.13). **Yours** to confirm or change.
- **`TAR_OPTIONS` and the command line** (phase 29, 2026-10-07): GNU tar drops the
  `TAR_OPTIONS` side of `--one-top-level` against `-P` given on the command line; cash
  refuses the pair wherever they come from.
- **zip -v's space before the tab** (phase 30, 2026-10-07): zip prints it where its
  progress dots would start; cash prints it for a stored member and for one of 64 KiB
  or more, which matched every case tried. A file between those may differ.
- **The first cash window after a Scoop update is blank for 1 to 3 seconds** (the user,
  2026-10-06); after that every start is fast. Measured here: a copy of `cash.exe` with
  a changed hash takes 1.2 to 2.3 s on its first run and about 50 ms after, which is
  Defender's cloud check of an unknown unsigned binary, cached by hash; 130 hard links
  to it, and a start in a new console, do not bring the check back. A forced reinstall
  of the same version (same hash) shows no delay. So the check is the likely cost, but
  the install's own three runs of the new exe (`--terminal-profile`, `--link-tools`,
  `--init-rc --once`) should already pay it, and cash keeps nothing per version (no
  cache on disk; checked). To settle: at the 1.5.0 update, time the first `cash -c true`
  and the first `cash -ic true` right after `scoop update cash`, then bisect. The
  lasting fix is code signing (ROADMAP 18, SignPath); a Defender exclusion needs admin.
  **Measured at the real 1.4.4 → 1.5.0 update (2026-10-06, idle machine):** first
  `cash -c true` 51 ms, first `cash -ic true` 76 ms, the second 69 ms. The shell's own
  first start after an update is as fast as any other, so the blank first tab is not
  cash's: it is Windows Terminal's first launch of a profile whose fragment was just
  rewritten, or Defender in Terminal's launch context. Left as is (the user: "we live
  with it"); if it is ever pursued, time a Terminal tab, not the shell.
- **The Scoop update output** (the user, 2026-10-06): the warning and the process table
  are Scoop's own (`test_running_process`, printed whenever a cash runs from
  `scoop\apps\cash`, which the window typing `scoop update` always does); only the notes
  are cash's. They now say the listed cash is that window; the bucket has them
  (pushed 2026-10-07).
- **zstd compresses at ruzstd's fast level only** (phase 28, 2026-10-07): every level
  `-1` to `-22` gives about zstd's `-1`, larger than zstd's default `-3`. When
  `libzstd-rs-sys` (Trifecta Tech's port of libzstd) has a Rust API, the levels can be
  real; until then the page says so.
- **Ten oracle tests keep their own copies of the oracle helpers** (found 2026-10-07,
  phase 28): `oracle_dir`, `run_oracle_script`, `golden` and `with_divergence` are
  written out again in `gzip_builtin.rs`, `grep_builtin.rs`, `column_builtin.rs` and
  seven more under `crates/cash/tests/it`. `common.rs` has them now, and the bzip2, xz
  and zstd tests use those; move the ten onto them.
- **CI's annotations show four `ENOENT ... opendir 'D:\a\cash\cash\target\doc\cash_core\tests\trybuild'`**
  (and `...\tests\target`) errors on every green run, 1.8.0's and 1.8.1's included
  (seen 2026-10-06). The job passes; the cleanup of the Rust Cache step
  (`Swatinem/rust-cache`, `.github/workflows/ci.yml`) walks `target\doc` and trips on
  folders that are gone. Find what makes `target\doc\cash_core\tests` and keep it out of
  the documentation, or exclude `target\doc` from the cache.
- **cash never runs a user's `command_not_found_handle`** (found building the install
  hint, 2026-10-06). Bash calls that function, when defined, with the command and its
  arguments instead of printing `command not found`, and its status becomes the
  command's. cash prints the message regardless; the hint is suppressed when the function
  is defined, as decided, but the function itself does not run. Implement Bash's
  behaviour, with the hint only when no function is defined.
- **`rm -rf` fails on a junction whose target it removed first** (found 2026-10-07,
  phase 31): in `t/` holding `sub/` and `jn` (a junction to `sub`), `rm -rf t` removes
  `sub` and then says `rm: cannot remove 't\jn': Permission denied`, leaving `t` and the
  dangling junction (status 1). `rm -rf t/jn` alone works and keeps `sub`. A junction
  left dangling seems to be removed as a file (`DeleteFileW`, refused) where it is a
  folder link (`RemoveDirectoryW`). Its `\` in the message is not cash's `/` either.
- **`pwd -W` is refused though the usage offers it** (found 2026-10-07): `pwd -W` says
  "-W: invalid option" then "usage: pwd [-LPW]". MSYS2's Bash has `-W` (the Windows
  path); either take it (cash's paths are Windows paths already) or drop it from the
  usage.
- A test in the full suite leaves `x.lnk` in the repository root: a 0-byte named pipe as
  MSYS2 makes them (its `mkfifo` writes a FIFO as a special `.lnk` file), timestamped
  during the 1.4.3 gate on 2026-10-06. No test names `x` with `mkfifo`; one of them runs
  with the repository as its folder. Find it and give it a temporary folder.

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
