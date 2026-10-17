# The parts called most: measured and made faster

The question (the user, 2026-10-17, after the shared parts): look deeply at the
components every builtin and command goes through many times (options, printing,
expansion, lookups and the like) for inefficient or unoptimized code. This note is what
was measured, what changed and what is left.

## Method

- **What:** 30 scripts, each a loop over one thing a script does often (arithmetic, a
  function call, `echo`, `printf`, `[ ]`, `[[ ]]`, `local`, a glob, `read` over a file,
  `type -P`, a command substitution, starting a program), and the builtins' own call
  cost (`:`, `true`, `shift 0`, `cd .`, `let`).
- **How:** hyperfine, five runs after a warm-up, cash built `--profile bench` (the
  release settings), each script under cash and under Git Bash 5's bash. An outlier
  against bash was looked into; each change was then timed against the binary before it.
- **No sampling profile:** wpr and xperf need elevation. Costs were found by splitting a
  benchmark into variants (`local x`, `local x=1`, `declare x=1` at top level, `export`)
  until one part stood out, then reading that code.

## Before: cash against bash

Most of the language is already ahead of bash (seconds, cash / bash):

| | cash | bash | |
|---|---|---|---|
| arithmetic, 200,000 `x=$((i*2+1))` | 0.50 | 0.96 | 0.52 |
| function calls, 100,000 | 0.42 | 0.79 | 0.53 |
| `s+=x`, 50,000 | 0.11 | 0.25 | 0.45 |
| `echo $i`, 100,000 | 0.42 | 0.51 | 0.81 |
| `[[ $i == 5 ]]`, 100,000 | 0.27 | 0.40 | 0.69 |
| starting `cmd.exe`, 100 | 2.09 | 4.83 | 0.43 |

Behind, and looked into:

| | cash | bash | |
|---|---|---|---|
| `local x=1` in a function, 50,000 | 1.23 | 0.46 | 2.69 |
| a glob of System32's DLLs, 200 | 2.19 | 1.08 | 2.03 |
| `type -P cmd`, 2,000 | 6.01 | 3.23 | 1.86 |
| `while read -r l` over 100,000 lines | 3.13 | 1.70 | 1.84 |
| `printf "%s %d\n" x $i`, 50,000 | 0.48 | 0.34 | 1.42 |
| `[ "$i" = 5 ]`, 100,000 | 0.79 | 0.61 | 1.31 |

Output, the buffer every builtin writes through, was already gathered into large writes
(an earlier pass), as were arithmetic's and patterns' parses, variable lookups and the
program cache: those showed no cost worth taking.

## What was slow, and what changed

| | before | after | bash |
|---|---|---|---|
| glob | 2.20 | 0.92 | 1.08 |
| `type -P` | 6.17 | 1.33 | 3.23 |
| `read` loop | 3.23 | 1.27 | 1.70 |
| `local x=1` | 1.21 | 0.30 | 0.46 |
| `local -i x=1` | 1.29 | 0.31 | |
| `[ ]` | 0.80 | 0.57 | 0.61 |

- **Globbing** made the pattern and its matcher again in every folder, read each name
  twice and sorted each folder's matches as paths, which compare part by part, before
  the whole expansion was sorted again as strings. Now once per component, the name read
  once, and the one sort.
- **A PATH search** looked at the name twice and at the name with each PATHEXT extension
  in every folder, about fifteen calls a folder. One directory query for the names that
  begin with the command's now tells which are there; what it finds is checked as
  before, and a name it cannot stand for is looked for as before. The first run of each
  command, `command -v` and `which` gain the same.
- **`read`** took a byte at a time, a system call a byte. Of a file on a disk it takes a
  block and moves the file back over what it read past the line, as Bash does with
  `lseek`; a pipe or a console still a byte at a time.
- **clap's parse** of a builtin's line sets and takes back a default for every option,
  whatever the line holds: two dozen for `declare`, a dozen for `read`. A builtin whose
  words repeat now gets back the parse of the last time they were the same
  (`builtins::parse_remembered`: `local x=1`, `local -i n`, `while read -r line`);
  `test` and `[`, which take no option, take their words as they are; and a builtin's
  clap command is built once a thread (worth 3 µs to `declare`, nothing to the small ones:
  the parse is the cost, not the building). Of `read`'s 20 µs a line, 7 were clap's.

## Left open

- `printf` was 1.4 times bash's time (6.3 µs a call against 2.2 for `true`): its
  arguments change from call to call, so its parse is not remembered. Dropping the 64 KiB
  buffer it makes for each call gained nothing measurable, and was left as it was. Its
  one option, `-v`, is parsed by hand since (2026-10-17), as Bash's `internal_getopt`
  takes it: 100,000 calls of `printf '%s %d\n' x "$i"` took 1.030 s, now 0.719 s,
  against 0.698 s for Git Bash's bash. uucore still parses the format on every call.
- The parse caches of words, tokens, patterns and `eval`'s programs are 64-entry LRUs
  behind a global lock, and each hit copies its key and its result: past 64 distinct
  words in a loop a word costs 1.8 µs against 1.3, and an `eval` in a loop copies its
  whole program. A thread's own map, larger, would take neither the lock nor the copies.
- `cd` takes 62 µs, against 2 to 4 for most builtins: `std::fs::metadata` opens a handle
  to tell a folder (`GetFileAttributesExW` would not), and the path is rendered twice.
  Bash's `cd` under MSYS takes 1.1 ms, so it is not behind.
