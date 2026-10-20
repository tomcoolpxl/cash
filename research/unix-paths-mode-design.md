# An opt-in `/c/` mode: design

The user's pick after 1.37.0 (decisions.md, "The phase after 1.37.0"): design a mode in
which cash spells paths as Git Bash does, `/c/Users/...`, before any code. This page is
the design; its open questions went to the user as a pick list.

**Decided** (the user, 2026-10-20, decisions.md D82): spelling and arguments as in 4.1,
switched on by a shell option that a child cash inherits through a variable, built in
the phase after 1.38.0.

## 1. Why

D3 prints every path as `C:/...`, because a printed path usually becomes an argument to a
Windows program, where `/c/src` is fatal; D4 never rewrites an argument; D5 translates
`PATH` alone. The corpus has found the price of that, in programs written for Unix:

| What the script does | Under `C:/...` | Found in |
| --- | --- | --- |
| climbs from `$PWD` with `dir=${dir%/*}` until `dir` is empty | `C:` has no `/` left, so it never ends: a hang | rbenv, pyenv, nvm |
| climbs with `dirname` until it reaches `/` | `dirname C:` is `C:`, in Git Bash too: a hang | asdf 0.15 |
| takes a path for absolute when it starts with `/` | `C:/...` is not, and the path is refused | bats' `load` (most bats suites), bash-oo-framework's `File::GetAbsolutePath` (falls back) |
| adds `$PWD` or `$HOME` to a colon-separated list | `C:` is cut off at its colon | nvm's `nvm_strip_path`, any `PATH="$PWD/bin:$PATH"` |
| uses `$PWD` in a `sed` address delimited by `:` | the colon ends the address | z's `-x` |
| runs `$(type -P tool)` unquoted for a program cash does not carry | Git's tools are under `C:/Program Files`, and the space splits the word | pyenv's `help` (`gawk`) |

Each case in the corpus works around its tool (a marker file at the top, a relative
`PATH` entry) or is known to differ (bats-assert). A user of those tools has no such
workaround: rbenv and nvm hang in a project directory with no version file above it.

## 2. How Git Bash does it

MSYS2, under Git Bash, keeps one POSIX view of the disks, `/c/...`, with a mount table
(`/usr/bin` is `C:/Program Files/Git/usr/bin`, `/tmp` the user's temporary directory), and
converts at the boundary to Windows programs:

- **Arguments.** For a program not linked against `msys-2.0.dll`, every argument that
  looks like a POSIX path is rewritten: `/c/x` to `C:/x`, `/usr/bin/x` through the mount
  table, `--opt=/c/x` after the `=`, and colon-separated lists (`/c/a:/c/b`) to `;`-lists.
  Its rules are heuristics, and they misfire: `cmd /c dir` loses its `/c`, `git log
  --format=/x` gets a path, a regular expression `/a/` becomes a file name. Hence the
  escapes: `MSYS_NO_PATHCONV=1` turns it off, `MSYS2_ARG_CONV_EXCL` excludes prefixes, and
  `//c` stands for a literal `/c`.
- **Environment.** Variables that look like paths or path lists are converted the same way
  for a Windows program (`PATH`, and any other whose value starts with `/`).
- **MSYS programs** (Git's `usr/bin`: `grep`, `sed`, `ls`) get POSIX paths unconverted, as
  they read them themselves.

## 3. What cash has already

- `C:/`, `/c/`, `/tmp` and `/dev` names open wherever cash opens a path itself (D3), so
  cash's builtins and bundled tools need no conversion in either spelling. (Wrong, as the
  build found: the bundled tools open what they are handed, and `cat /c/x` failed in
  either mode. The user's pick, 2026-10-20, in D82: they read `/c/` as an MSYS tool does,
  in either mode.)
- `cash_win32::path::unix_drive_spelling` turns `/c/x` into `C:/x`; the warning cash gives
  for a `/c/` argument to a program uses it.
- `cash_win32::msys::is_msys_program` tells an MSYS2 or Cygwin program by its import table;
  cash already quotes their arguments by Cygwin's rules.
- `PATH` is shown as `/c/...` and converted at the boundary (D5).
- `/usr/bin/NAME` runs a command cash carries (`system_tool`), and `type -P` prints it.

## 4. The design

### 4.1 What the mode changes

One switch, `shopt -s unixpaths` (name to be chosen), off by default. On:

1. **Spelling.** Every path cash builds or prints is rendered `/c/...`: `pwd`, `$PWD`,
   `$OLDPWD`, `$HOME`, `~`, `dirs`, globs that start from an absolute path, `$BASH_SOURCE`,
   `$0` of a script run by its path, `realpath` and `readlink -f`, `which` and `type -P`.
   One function renders a path for display today (`cash_win32::path::render`); the mode is
   a switch there, so that no caller decides on its own (central, as the user asks).
2. **Arguments to Windows programs.** For an external program that is not an MSYS program,
   cash rewrites, as MSYS does, the arguments that are POSIX absolute paths it can map: a
   whole word `/c/...`, `/tmp/...`, `/usr/bin/...` (to the tool's real path, or left if
   cash carries it), and the value after `=` of `--opt=/c/...` and `NAME=/c/...`. No list
   splitting and no other heuristics: a word that starts with `/` and names no drive
   (`/a/`, `/c` alone, `/q`) is left as it is, so `cmd /c` and a regular expression
   survive. `//c` stands for a literal `/c`, as in MSYS.
3. **Environment for Windows programs.** `PATH` as today (D5), and `HOME`, `PWD` and
   `OLDPWD` in the Windows form, as they are the variables the mode itself changed. Others
   pass as they are (D5's rule stands).
4. **MSYS programs and cash's own** get every argument as written.

D3 stays the default; D4 holds whenever the mode is off. With it on, D4 gives way only
for what item 2 lists, and a warning is no longer needed for `/c/` arguments.

### 4.2 How it is switched on

- `shopt -s unixpaths` in `~/.bashrc` or a script, inherited by a child cash through an
  environment variable (`CASH_UNIXPATHS=1`), so that `bash script.sh` started from a shell
  in the mode is in it too, as `SHELLOPTS` carries `set -o` options.
- The variable alone, for a session or a CI job, without touching scripts.

### 4.3 What stays out

- Mounts other than `/c/` drives, `/tmp` and `/usr/bin`: no `/etc` or `/home` table.
- Converting variables other than item 3's.
- Changing D3's default.

## 5. Risks

- A Windows program that takes options beginning with `/` and a drive letter, `/c/...`
  meant literally: rare; `//c` writes it.
- Scripts that test `[[ $PWD == C:/* ]]` for cash: they would see `/c/`; the switch is
  theirs to set.
- Output compared with a Windows program's own output (`git rev-parse --show-toplevel`
  prints `C:/...`): in Git Bash it is the same mismatch, and scripts written for Git Bash
  already cope.
- The corpus: every case runs in both modes in the golden test, or the mode gets cases of
  its own; the workarounds in rbenv's, pyenv's, nvm's, asdf's and bats-assert's cases are
  dropped in the mode's copies.

## 6. Size

- Rendering through one switch: small, if every printed path goes through `render`; a
  sweep finds those that do not.
- Argument rewriting for non-MSYS programs: medium, at the one place a Windows command line
  is built (`compose_std_command`), with tests for each rule and each exception.
- The shopt, the variable and inheritance: small.
- Tests: the climbing tools and bats `load` run unchanged in the mode; a test per argument
  rule; the corpus in the mode.

In all, medium to large: about a phase.

## 7. Open questions, for the pick list

1. What the mode rewrites: spelling only (D4 kept; Windows programs then get `/c/` paths
   from `$PWD` and fail), spelling and arguments as in 4.1 (recommended), or also every
   variable that looks like a path, as MSYS does.
2. How it is switched on: a shopt inherited through a variable (recommended), the variable
   alone, or both a shopt and `set -o`.
3. When to build it: the next phase, or parked until a user asks.
