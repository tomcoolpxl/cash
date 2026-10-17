# cash-uutils

`rm`, `sort`, `tee`, `uniq`, `shuf` and `head` for cash: taken from
[uutils coreutils](https://github.com/uutils/coreutils) 0.12.0 (`uu_rm`, `uu_sort`,
`uu_tee`, `uu_uniq`, `uu_shuf`, `uu_head`), © the uutils developers, under the MIT
License (`LICENSE`), and made cash's own on 2026-10-10 (`head` on 2026-10-17). Until
then the first five were carried in `vendor/uutils` as patched copies, and `head` was
used from crates.io as published, as the other uutils tools cash bundles still are.

Each tool is a module with its own `uumain`, which `cash-coreutils-builtins` registers
as it registers the uutils tools it uses unchanged.

## What changed on the way in

- **Messages.** Each tool keeps its `en-US.ftl`, compiled in, and its `translate!`
  calls; `src/messages.rs` looks an id up in the tool's own file, then in uucore's. In
  uutils a tool's messages come from files uucore's build script finds in the cargo
  registry under `uu_<tool>-<version>`, which a tool built from anywhere else does not
  have on a clean machine: releases 1.10.0 and 1.11.0 printed `sort: sort-cannot-read`
  and `Usage: tee-usage`. Other languages' files were left out: cash speaks English.
- **Windows only.** Code for Unix, Linux, Redox and WASI is gone (`rm`'s safe
  traversal, `tee`'s splicing and raw writes, `sort`'s resource limits, file
  descriptor counts, permissions and single-threaded path), with the tests of it, and
  so are the binaries' `main.rs` files. `sort` always had uucore's `i18n-collator`; its
  feature switch went, the collator kept.
- **A `>(...)` is opened as the pipe it is** (2026-09-30, then a patch). cash hands a
  `>(...)` to `tee`, `sort`, `uniq` and `shuf` as a named pipe (spec D17), which cannot
  be created, truncated or appended to: `echo x | tee >(cat)` failed with "The parameter
  is incorrect". Each opens its output through `cash_win32::pipe::open_output`, and
  `sort` leaves such a pipe alone when it would truncate its output. Marked `// cash:`.
- **`rm -r` removes a folder link inside a folder as one** (2026-10-10, then a patch): a
  junction or a symbolic link to a folder is taken away with `RemoveDirectoryW`, never
  followed, whether its target is there or not; `DeleteFileW` refused it ("Permission
  denied").
- **What uutils asks of Unix alone, asked of Windows** (2026-10-10): `sort` sizes its
  buffer from the memory Windows can hand out (`GetPerformanceInfo`), as uutils takes
  `MemAvailable` on Linux; it had the fixed 32 MiB. `tee -i` ignores Ctrl-C, Windows'
  SIGINT, with a console handler that the programs it starts do not inherit.
- **`rm`'s trailing separators**: `d/../////` is trimmed to `d/../` with either
  separator; uutils looked for `\` alone on Windows.
- **`head` leaves a file under its standard input just after what it printed**
  (2026-10-17), as GNU head does: `{ head -n1; cat; } < file` lost what head had read
  ahead, as uutils seeks back on Unix alone. A head that comes to the file under way
  reads what is left of it, also for `-n -N` and `-c -N` on a large file, which uutils
  measured from the file's start. A pipe or a console is read as before. Marked
  `// cash:`.
- **What could panic** now says why it cannot, or is an error the tool reports, under
  the workspace's lints:
  - `sort`: a temporary file that cannot be written says `write failed: PATH: REASON`
    with status 2, as GNU sort does; a chunk reader whose receiver has gone stops;
    two `-g` values that do not compare (NaN) are equal rather than a panic; an empty
    file list is empty input; a poisoned lock is taken over.
  - `shuf`: an empty list to choose from is `no lines to repeat`.
  - `rm`: a progress template that will not parse falls back to the default bar.
  - `head`: the loop that holds lines back copies everything when it is to hold none
    back, where it asserted that it never was (its caller sees to `-n -0`), and looks
    for a line's end where it assumed one; the buffer it searches a file's end in is
    on the heap, not 64 KiB of stack.
- **Style.** The workspace's style lints that uutils was not written to are allowed in
  `src/lib.rs`, as for `cash-sed`; the lints for code that can panic apply in full.
