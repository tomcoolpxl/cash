# cash-uutils

`rm`, `sort`, `tee`, `uniq`, `shuf`, `head` and `tail` for cash: taken from
[uutils coreutils](https://github.com/uutils/coreutils) 0.12.0 (`uu_rm`, `uu_sort`,
`uu_tee`, `uu_uniq`, `uu_shuf`, `uu_head`, `uu_tail`), © the uutils developers, under
the MIT License (`LICENSE`), and made cash's own on 2026-10-10 (`head` and `tail` on
2026-10-17). Until then the first five were carried in `vendor/uutils` as patched
copies, and `head` and `tail` were used from crates.io as published, as the other
uutils tools cash bundles still are.

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
  descriptor counts, permissions and single-threaded path, `head`'s and `tail`'s
  splicing, `tail`'s non-blocking FIFO open and its watch of a file's directory on Linux),
  with the tests of it, and so are the binaries' `main.rs` files. `sort` always had
  uucore's `i18n-collator`; its feature switch went, the collator kept. `tail`'s one
  rstest table is a loop over the same cases.
- **A `>(...)` is opened as the pipe it is** (2026-09-30, then a patch). cash hands a
  `>(...)` to `tee`, `sort`, `uniq` and `shuf` as a named pipe (spec D17), which cannot
  be created, truncated or appended to: `echo x | tee >(cat)` failed with "The parameter
  is incorrect". Each opens its output through `cash_win32::pipe::open_output`, and
  `sort` leaves such a pipe alone when it would truncate its output. Marked `// cash:`.
- **`rm -r` removes a directory link inside a directory as one** (2026-10-10, then a patch): a
  junction or a symbolic link to a directory is taken away with `RemoveDirectoryW`, never
  followed, whether its target is there or not; `DeleteFileW` refused it ("Permission
  denied").
- **What uutils asks of Unix alone, asked of Windows** (2026-10-10): `sort` sizes its
  buffer from the memory Windows can hand out (`GetPerformanceInfo`), as uutils takes
  `MemAvailable` on Linux; it had the fixed 32 MiB. `tee -i` ignores Ctrl-C, Windows'
  SIGINT, with a console handler that the programs it starts do not inherit.
  `tail -f` asks whether its standard input is a file before it warns that following it
  is ineffective, as GNU tail does (uutils took any input for one), and `--pid` holds
  the process through `cash_win32::process::Held`. `tail` no longer ends itself with 13
  on a broken pipe: cash's relay of a bundled tool's output ends it with 141.
- **`rm`'s trailing separators**: `d/../////` is trimmed to `d/../` with either
  separator; uutils looked for `\` alone on Windows.
- **`rm` removes the directory it runs in** (2026-10-17): the bundled `rm` runs in the
  shell's directory, and Windows will not remove a directory a process sits in, so
  `d=$(mktemp -d); trap 'rm -rf "$d"' EXIT; cd "$d"` left `$d` behind (spec §4 #13).
  Before it removes a directory that holds its own, `rm` moves to the volume's root, and a
  relative operand goes on as its full path; a relative operand after that names
  nothing, as on Linux once the working directory is gone. An operand ending in `.` or
  `..` is still refused. Marked `// cash:` (`leave_directory_for`).
- **`head` leaves a file under its standard input just after what it printed**
  (2026-10-17), as GNU head does: `{ head -n1; cat; } < file` lost what head had read
  ahead, as uutils seeks back on Unix alone. A head that comes to the file under way
  reads what is left of it, also for `-n -N` and `-c -N` on a large file, which uutils
  measured from the file's start. A pipe or a console is read as before. Marked
  `// cash:`.
- **`tail` gives the end of a pipe, and of a file of whole 64 KiB blocks** (2026-10-17),
  two bugs of uutils': `head -n -1 big | tail -c 8` printed 2,000 to 10,000 bytes, a
  different count each run, as `tail -c` dropped one unneeded chunk per read and a pipe
  hands reads over in mixed sizes; `tail -n 2` printed nothing of a file of 64 KiB, and
  `tail -n 5000` gave 4096 lines of one of 128 KiB, as the last block read back from the
  end was taken to be the size's remainder, 0. Marked `// cash:`.
- **`tail -f --use-polling` follows a file without printing it again** (2026-10-17):
  `Metadata` has no file index on Windows without an unstable API, so uutils could not
  tell a file from its replacement and, polling, took every change for a new file
  (`has been replaced;  following new file`, and the file again from its start). The
  identity (`cash_win32::fs::file_info`'s volume and index) is kept beside the metadata.
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
  - `tail`: a seek or a read of a file that fails is an error; a byte count past what a
    seek takes is the whole file, where a cast wrapped it to a seek past the end; an
    event for a path not followed, or without its new name, changes nothing; a watcher
    that was not started is an error.
- **`--version` names uutils' 0.12.0** (2026-10-17), as the bundled tools do: uucore's
  `crate_version!` gave this crate's version, `sort (uutils coreutils) 1.19.0`, a uutils
  release there never was.
- **Style.** The workspace's style lints that uutils was not written to are allowed in
  `src/lib.rs`, as for `cash-sed`; the lints for code that can panic apply in full.
