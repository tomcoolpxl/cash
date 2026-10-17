# Benchmarks at 1.23.0, against the last and the first

The question (the user, 2026-10-17, after 1.23.0): a benchmark of 1.23.0 against the last
big one and against the first one, a few days before, step by step, to see what the
speed work changed.

## Method

- **The two sets:** `tests/shell-bench` (ten workloads, cash, Git Bash 5.3, PowerShell 7
  and WSL's bash, hyperfine's median of at least ten runs, one iteration's cost with the
  shell's start taken off) and the 30-script set of `research/hot-paths.md` (cash and Git
  Bash's bash, the mean of five runs after a warm-up).
- **The builds:** 1.23.0 built `--profile bench` (the release settings; the shipped
  binary is `dist`, fat LTO). The first run used Scoop's 1.18.0 (`dist`), the last big
  run a bench build of 2026-10-16 20:00, the hot-path baseline this morning's.
- **The control:** Git Bash is in every run, and its software did not change: what moves
  in its column is the machine. It stayed within ±7% except where said.

## 1. 1.23.0 (shell-bench)

| workload | cash | Git Bash | PowerShell 7 | WSL bash |
|---|---|---|---|---|
| startup | 20.7 ms | 26.6 ms | 261.1 ms | 3.4 ms |
| empty loop | 2.2 us | 3.5 us | 1.7 us | 1.5 us |
| `echo hi` | 3.0 us | 4.3 us | 26.6 us | 2.3 us |
| arithmetic | 2.5 us | 5.0 us | 2.1 us | 2.1 us |
| expansions | 7.8 us | 10.1 us | 8.5 us | 4.7 us |
| function call | 4.1 us | 7.4 us | 15.4 us | 2.8 us |
| `x=$(echo hi)` | 66.6 us | 21.82 ms | 111.3 us | 617.4 us |
| start a program | 19.64 ms | 39.47 ms | 18.82 ms | 0.86 ms |
| `echo hi \| cat` | 18.73 ms | 48.70 ms | 0.52 ms | 1.56 ms |
| `ls -la` of 210 entries | 15.62 ms | 49.12 ms | 49.34 ms | 3.92 ms |

cash is ahead of Git Bash in every row, and of PowerShell in all but the loop,
arithmetic, a program's start (equal) and its in-process "pipe".

## 2. Against the last big run

### shell-bench, against 2026-10-16 20:00

| workload | then | 1.23.0 | change | Git Bash then | now |
|---|---|---|---|---|---|
| startup | 21.5 ms | 20.7 ms | 4% faster | 27.2 ms | 26.6 ms |
| empty loop | 2.3 us | 2.2 us | 4% faster | 3.6 us | 3.5 us |
| `echo` | 3.1 us | 3.0 us | 4% faster | 4.2 us | 4.3 us |
| arithmetic | 2.5 us | 2.5 us | same | 5.0 us | 5.0 us |
| expansions | 10.9 us | 7.8 us | 29% faster | 10.7 us | 10.1 us |
| function call | 4.2 us | 4.1 us | same | 7.5 us | 7.4 us |
| `$(...)` | 82.6 us | 66.6 us | 19% faster | 20.50 ms | 21.82 ms |
| program | 20.28 ms | 19.64 ms | 3% faster | 41.42 ms | 39.47 ms |
| pipe | 19.14 ms | 18.73 ms | same | 49.45 ms | 48.70 ms |
| `ls -la` | 13.46 ms | 15.62 ms | **16% slower** | 52.68 ms | 49.12 ms |

### The hot paths, against this morning's baseline (seconds)

| script | before | 1.23.0 | change | bash now | cash/bash |
|---|---|---|---|---|---|
| `type -P cmd`, 2,000 | 6.014 | 1.295 | 4.6x faster | 3.280 | 0.39 |
| `while read -r` over 100,000 lines | 3.131 | 1.224 | 2.6x faster | 1.686 | 0.73 |
| a glob of System32's DLLs, 200 | 2.192 | 0.924 | 2.4x faster | 1.081 | 0.85 |
| `printf "%s %d\n"`, 50,000 | 0.478 | 0.355 | 26% faster | 0.340 | 1.04 |
| `grep -c` of a file | 0.030 | 0.025 | 17% faster | 0.059 | 0.42 |
| `seq \| wc -l` | 0.065 | 0.059 | 9% faster | 0.088 | 0.67 |
| `[[ ]]`, `[ ]` | failed | 0.289, 0.584 | - | 0.388, 0.607 | 0.74, 0.96 |

The other 17 within 5% of before (arithmetic, calls, `echo`, expansions, arrays, `cat`,
`sed | awk`, a program's start and the rest). Five were behind bash this morning; now
only `printf` is level with it (1.04), and every other one is ahead.

## 3. Against the first run (shell-bench, 2026-10-16 13:08, Scoop's 1.18.0)

| workload | 1.18.0 | 1.23.0 | change | Git Bash then | now |
|---|---|---|---|---|---|
| startup | 36.9 ms | 20.7 ms | 44% faster | 26.8 ms | 26.6 ms |
| empty loop | 7.0 us | 2.2 us | 3.2x faster | 3.9 us | 3.5 us |
| `echo` | 16.3 us | 3.0 us | 5.4x faster | 4.3 us | 4.3 us |
| arithmetic | 7.3 us | 2.5 us | 2.9x faster | 4.9 us | 5.0 us |
| expansions | 20.1 us | 7.8 us | 2.6x faster | 10.1 us | 10.1 us |
| function call | 11.6 us | 4.1 us | 2.8x faster | 7.3 us | 7.4 us |
| `$(...)` | 178.2 us | 66.6 us | 2.7x faster | 17.77 ms | 21.82 ms |
| program | 18.14 ms | 19.64 ms | 8% slower | 38.95 ms | 39.47 ms |
| pipe | 24.73 ms | 18.73 ms | 24% faster | 45.31 ms | 48.70 ms |

The first run had no `ls -la` row; in the A/B below, 1.18.0 took 4.2 s for it (the folder
link count read the temporary folder's subfolders, fixed on 2026-10-16), 1.23.0 0.8 s.

## The two that went backwards, A/B in one sitting

Each build run side by side (hyperfine, at least ten runs; the median):

| build | `ls -la` × 50 | `hostname.exe` × 100 |
|---|---|---|
| 1.18.0 (`dist`) | 4185 ms | 1988 ms |
| 2026-10-16 20:00 | 697 ms | 2082 ms |
| 2026-10-17 16:25 | 676 ms | 1951 ms |
| cd4283c7 (`ls`'s times and sizes) | 712–722 ms | |
| 62af986b (before the access time) | 731 ms | |
| 1.23.0 | 813–840 ms | 1960 ms |

- **A program's start did not go backwards:** side by side, 1.23.0 starts 100 programs
  in 1960 ms, 1.18.0 in 1988. The 8% of step 3 was the two days' machines.
- **`ls -la` did, by 20%, in two steps:** `ls`'s times and sizes (cd4283c7) read each
  file's facts for the totals' allocation, 7%; and keeping a file's access time while it
  is read for `#!` (8df41197), an open with `FILE_WRITE_ATTRIBUTES` and a `SetFileTime`
  for each such file, about 8 µs a file, 12%. GNU's ls reads no file, and Git Bash's
  moves the access time as cash did before.
