# cash-uutils

`cat`, `cp`, `head`, `ln`, `mkdir`, `mv`, `paste`, `pr`, `ptx`, `rm`, `rmdir`, `shred`,
`shuf`, `sort`, `split`, `tac`, `tail`, `tee`, `touch` and `uniq` for cash: taken from
[uutils coreutils](https://github.com/uutils/coreutils) 0.12.0 (`uu_cat`, `uu_cp`,
`uu_head`, `uu_ln`, `uu_mkdir`, `uu_mv`, `uu_paste`, `uu_pr`, `uu_ptx`, `uu_rm`,
`uu_rmdir`, `uu_shred`, `uu_shuf`, `uu_sort`, `uu_split`, `uu_tac`, `uu_tail`,
`uu_tee`, `uu_touch`, `uu_uniq`), © the uutils developers, under the MIT License (`LICENSE`), and
made cash's own: `rm`, `sort`, `tee`, `uniq` and `shuf` on 2026-10-10, `head` and `tail`
on 2026-10-17, eleven more on 2026-10-18 to say their errors in GNU's words, and
`split` on 2026-10-19 for `--filter` on Windows, and `touch` on 2026-10-21 to go on past
an operand it cannot touch. Until
then the first five were carried in `vendor/uutils` as patched copies, and the rest
were used from crates.io as published, as the other uutils tools cash bundles still
are.

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
  splicing, `tail`'s non-blocking FIFO open and its watch of a file's directory on Linux,
  `cp`'s copy-on-write for Linux and macOS, SELinux and SMACK, `mv`'s hard-link
  tracking, `mkdir`'s umask and modes, `cat`'s raw writes),
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
- **`split --filter` on Windows** (2026-10-19): uutils refused it there, as its Unix
  writer set `FILE` for the whole process before starting the shell. Each piece is now
  piped into cash itself, this process's own exe, with `--norc --noprofile -c COMMAND`,
  `FILE` set for that child alone and `CASH_ARGV0` telling a linked `split.exe` it was
  started as the shell, as for awk's `system()`. Its Unix and WASI writers are gone.
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
- **`rm -i` asks in GNU's words** (2026-10-18, found writing rm's man page): `remove
  regular file 'a.txt'?`, as GNU's `rm` asks and the prompts beside it did; uutils
  asked `remove file`. Marked `// cash:`.
- **Errors in GNU's words** (2026-10-18). An error from Windows is worded as glibc's
  `strerror` words it, `No such file or directory`, through `cash_win32::errno`, which
  maps each Windows code to the errno Cygwin gives it; uutils printed Windows' text
  ("The system cannot find the file specified.") wherever it did not go through
  uucore's `UIoError`, and uucore's `strip_errno` only cut off `(os error 2)`. Every
  `strip_errno` here is `cash_win32::errno::strerror`, in `sort` and `tee` too.
- **Messages shaped as GNU's** (2026-10-18), each checked against Git Bash 5.3:
  `mkdir: cannot create directory 'a': File exists` (uutils: `a: File exists`); `cp:
  cannot stat 'x'` for a source that is not there (uutils looked for Linux's words to
  know it), `cannot open 'x' for reading` and `cannot create regular file 'y'` (uutils:
  `'x' -> 'y': ...`); `mv: cannot move 'a' to 'b': ...` (uutils: Windows' text alone);
  `ln: failed to access 'x'` for a hard link's missing source, and a link's name joined
  to its directory with `/`; `paste: x: ...` with the file named; `ptx` quoting a name
  only when it needs it; `shred: x: failed to open for writing: ...`. Marked `// cash:`.
- **`mv` copies only across drives, or onto a target another program holds**
  (2026-10-18): uutils fell back to copying and deleting on any failed rename, so `mv a
  nodir/b` copied, failed again, and said `Permission denied`; and it called every
  failure of that fallback `Permission denied`. The fallback keeps its own errors.
- **`cp` dates a copy now** (2026-10-18): Windows' `CopyFileExW`, under `fs::copy`,
  gives a copy its source's modification time; GNU's `cp` gives it the present time
  unless `-p`, `-a` or `--preserve=timestamps` keeps the source's, and so does this
  one (`date_copy_now`). `cp -p` of a read-only file sets its times with the read-only
  attribute lifted for the moment (`set_times_of_copy`): Windows sets no time on a
  read-only file, so it made the copy and failed with `Access is denied`.
- **`ln -s` makes a junction for a directory without the privilege** (2026-10-18,
  spec D27): a symbolic link needs Developer Mode or an elevated shell; for a directory
  a junction needs neither. A symbolic link to a file that Windows refuses says how to
  get one. The target of a relative link is looked at from the link's directory, where
  uutils looked from the working directory.
- **`-i` answered no exits 1**, in `cp`, `mv` and `ln`, as GNU coreutils 9 does; Git
  Bash's coreutils 8.32 exits 0.
- **`touch` goes on past an operand it cannot touch** (2026-10-21), as GNU touch does:
  one whose times could not be read, as a name Windows refuses (`touch x 'a"b' y`),
  ended the run with its error and `y` was never made. Each is said and the rest are
  touched, status 1. Marked `// cash:`.
- **Style.** The workspace's style lints that uutils was not written to are allowed in
  `src/lib.rs`, as for `cash-sed`; the lints for code that can panic apply in full:
  each unwrap and slice the copied code had is rewritten, or says why it cannot fail
  (`#[expect(..., reason)]`), and the large buffers are on the heap.
