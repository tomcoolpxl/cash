---
name: ulimit
summary: Show resource limits, as far as Windows has them.
kind: Bash builtins
see: differences
draft: yes
---

## How it works

`ulimit` shows or sets resource limits: `-n` open files, `-u` processes, `-a` all.

## Different in cash

Windows has no per-process limits of this kind; its job objects limit other things, per
job. So `ulimit` reports `unlimited`, which is true of what scripts ask about (there is
no cap on open files), and accepts a setting without applying it, so the common `ulimit
-n 4096` line does not stop a script under `set -e`. Background jobs and subshells are
limited to 256 at once, or `CASH_MAX_SUBSHELLS`.
