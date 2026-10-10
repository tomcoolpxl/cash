# cash-uutils

`rm`, `sort`, `tee`, `uniq` and `shuf` for cash: taken from
[uutils coreutils](https://github.com/uutils/coreutils) 0.12.0 (`uu_rm`, `uu_sort`,
`uu_tee`, `uu_uniq`, `uu_shuf`), © the uutils developers, under the MIT License
(`LICENSE`), and made cash's own on 2026-10-10. Until then they were carried in
`vendor/uutils` as patched copies; the other uutils tools cash bundles are used from
crates.io as published.

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
- **`rm`'s trailing separators**: `d/../////` is trimmed to `d/../` with either
  separator; uutils looked for `\` alone on Windows.
- **What could panic** now says why it cannot, or is an error the tool reports, under
  the workspace's lints:
  - `sort`: a temporary file that cannot be written says `write failed: PATH: REASON`
    with status 2, as GNU sort does; a chunk reader whose receiver has gone stops;
    two `-g` values that do not compare (NaN) are equal rather than a panic; an empty
    file list is empty input; a poisoned lock is taken over.
  - `shuf`: an empty list to choose from is `no lines to repeat`.
  - `rm`: a progress template that will not parse falls back to the default bar.
- **Style.** The workspace's style lints that uutils was not written to are allowed in
  `src/lib.rs`, as for `cash-sed`; the lints for code that can panic apply in full.
