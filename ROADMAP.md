# Cash roadmap

This is the central ordering document for planned compatibility and bundled-userland
work. Detailed investigations and implementation checklists remain in `research/`, but
their priority is set here.

Urgent regressions, crashes, security problems, and failing CI may interrupt this order.
Feature work follows the sequence below.

## Current sequence

| Order | Workstream | Status | Detailed source |
| ---: | --- | --- | --- |
| 1 | Finish the advertised Bash 5.2 interface | **Active** | [Bash 5.2 remaining plan](research/bash-reference/bash-5.2-remaining-plan.md) and [gap audit](research/bash-reference/bash-5.2-gaps.md) |
| 2 | Absorb and integrate a native `awk` | **Queued after Bash 5.2** | [`quinnjr/rawk` evaluation](research/rawk-evaluation.md) |
| 3 | Bash 5.3 compatibility work | **Parked; last** | Selected existing behavior is recorded in the [Bash source comparison](research/bash-reference/README.md#bash-53-selected-features-not-a-version-claim) |

The authoritative feature order is therefore:

```text
Bash 5.2 completion  ->  native awk  ->  Bash 5.3
```

## 1. Finish Bash 5.2

Cash reports `BASH_VERSION=5.2.37`, so closing or explicitly classifying its 5.2 gaps is
the first feature priority. Work through the existing plan in its own phase order:

1. small portable semantics: here-documents, `ulimit` parsing, `command -p`, startup
   `$0`, empty-word descriptor duplication, and fatal parameter transformations;
2. array, subscript, `unset`, and nameref evaluation edge cases;
3. completion and interactive editing, including globstar completion, `read -E`, history
   navigation, and the separately scoped custom-keymap work;
4. variable file-descriptor redirection and `varredir_close`;
5. the Bash 5.2 POSIX-mode pass;
6. a Linux GitHub Actions differential job using an actual Bash 5.2 executable, while
   retaining native Windows coverage for CRLF, ConPTY, paths, and handles.

The [remaining plan](research/bash-reference/bash-5.2-remaining-plan.md) owns the probe
matrix and implementation sequence. The [gap audit](research/bash-reference/bash-5.2-gaps.md)
owns observed results and status.

This workstream is complete when every listed 5.2 NEWS item and older advertised-interface
gap is recorded as one of:

- verified compatible with a focused regression;
- fixed with a focused regression;
- a documented, deliberate Windows divergence;
- a non-applicable Bash build or library detail.

“Needs investigation” does not count as complete. A deliberate platform divergence may
close an item; exact Unix behavior is not required where Win32 lacks the underlying
concept.

## 2. Add native `awk`

Start this only after the Bash 5.2 completion gate above. Use the reviewed
[`quinnjr/rawk`](https://github.com/quinnjr/rawk) source as a one-time import and maintain
the resulting code inside Cash. Do not create an upstream-tracking fork or Git dependency.

The implementation sequence is:

1. create a private `crates/cash-awk` crate, preserve the MIT notice and imported revision,
   and retain the useful upstream tests;
2. register `awk` through Cash's process-backed bundled-command shim so pipelines,
   redirections, Ctrl-C, job control, working directory, and environment use Cash's normal
   execution path;
3. replace all hardcoded `sh -c` paths used by `system()`, input command pipes, and output
   command pipes with a Cash command host;
4. fix the verified compatibility blockers: arbitrary single-character `RS`, POSIX
   leftmost-longest ERE behavior, repeated `-f`, `-f -`, interspersed operand assignments,
   and input selection through `ARGC`/`ARGV`;
5. remove or contain the lifetime-erased input pointer and convert subprocess-handle
   `unwrap()` calls into reported errors;
6. add Windows and Linux differential tests against known AWK implementations;
7. register the command as `awk`, update `cash doctor`, and change README/spec statements
   that currently say Cash does not bundle AWK.

The detailed evidence and design are in
[research/rawk-evaluation.md](research/rawk-evaluation.md). The command may ship when its
POSIX core passes the focused differential suite. Complete gawk compatibility, two-way
pipes, network pseudo-files, and `@include` are outside the initial gate.

## 3. Bash 5.3 comes last

Cash already contains a few selected Bash 5.3 behaviors, including current-shell command
substitution, `compgen -V`, and `source -p`. They remain supported, but they do not change
the ordering or constitute a Bash 5.3 version claim.

After Bash 5.2 and `awk` are complete, create a dedicated Bash 5.3 NEWS/source audit before
adding more features. Classify the results into:

1. portable language and builtin behavior;
2. interactive completion, Readline, and job-control behavior;
3. POSIX-mode behavior changes;
4. Bash build internals that do not create a Cash-visible contract;
5. deliberate Win32 divergences.

Prioritize small, useful script-visible features that fit Cash's architecture. Larger
POSIX-mode or editor changes require focused designs and differential tests. Do not report
`BASH_VERSION=5.3` until the audit is complete and every applicable item has a final
classification.

## Keeping the roadmap current

When work starts or finishes:

1. update the status table in this file;
2. update the linked detailed plan or audit with probes, implementation status, and tests;
3. update README/spec only when user-visible behavior actually changes;
4. move newly discovered work into the applicable detailed plan instead of leaving it in
   chat history or an isolated note.

This file owns **what comes next**. The linked documents own **how each workstream is
implemented and verified**.
