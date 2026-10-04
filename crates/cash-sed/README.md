[![Crates.io](https://img.shields.io/crates/v/sed.svg)](https://crates.io/crates/sed)
[![Discord](https://img.shields.io/badge/discord-join-7289DA.svg?logo=discord&longCache=true&style=flat)](https://discord.gg/wQVJbvJ)
[![License](http://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/uutils/sed/blob/main/LICENSE)
[![dependency status](https://deps.rs/repo/github/uutils/sed/status.svg)](https://deps.rs/repo/github/uutils/sed)

[![CodeCov](https://codecov.io/gh/uutils/sed/branch/main/graph/badge.svg)](https://codecov.io/gh/uutils/sed)

# sed

Rust reimplementation of the [sed utility](https://pubs.opengroup.org/onlinepubs/9799919799/utilities/sed.html)
with some [GNU sed](https://www.gnu.org/software/sed/manual/sed.html),
[FreeBSD sed](https://man.freebsd.org/cgi/man.cgi?sed(1)),
and other extensions.

## In Cash

This crate is Cash's copy of [uutils/sed](https://github.com/uutils/sed). The rest of
this README is upstream's: its installation, release and test instructions describe the
upstream project, not this crate. Cash's changes from upstream are listed in
[research/uutils-sed-evaluation.md](../../research/uutils-sed-evaluation.md).

Cash builds for Windows only, so the Unix-only code paths were removed: the mmap(2) input
and write(2)/copy_file_range(2) zero-copy output in `fast_io.rs`, the `/bin/sh` fallback
for `e` and `s///e`, copying Unix permission bits on in-place edits, and the Unix-only tests.

## Status

At this state _sed_ implements all [POSIX features](https://pubs.opengroup.org/onlinepubs/9799919799/)
and can run correctly the three complex scripts of its integration tests:
[hanoi.sed](https://github.com/uutils/sed/blob/main/tests/fixtures/sed/script/hanoi.sed) (solves the Towers of Hanoi puzzle),
[mandelbrot.sed](https://github.com/uutils/sed/blob/main/tests/fixtures/sed/script/mandelbrot.sed), (draws the Mandelbrot set) and
[math.sed](https://github.com/uutils/sed/blob/main/tests/fixtures/sed/script/math.sed)  (implements an arbitrary precision integer math calculator).

The performance of this Rust implementation is now better than the GNU and FreeBSD implementations for most benchmarked cases.

Further work aims to:
* improve GNU _sed_ compatibility, especially on the regular expression front,
* implement more GNU extensions, and
* improve performance where possible.

## Installation and Use

We provide a Linux x86_64 binary archive from the main branch at
https://github.com/uutils/sed/releases/tag/latest-commit .

If you have [cargo-binstall](https://github.com/cargo-bins/cargo-binstall),
the released binaries can be installed directly with:

```bash
cargo binstall sed
```

For other platforms, ensure you have Rust installed on your system. You can install Rust through [rustup](https://rustup.rs/).

Clone the repository and build the project using Cargo:

```bash
git clone https://github.com/uutils/sed.git
cd sed
cargo build --release
cargo run --release
```

The binary is named `sed` in `target/release/sed`.

You can also try *sed* on the web
through the [uutils Playground](https://uutils.org//playground/)
by clicking on the `Load sed` button.

## Testing

### GNU sed Compatibility Testing

Test compatibility against GNU sed by running the upstream testsuite shell scripts
with a lightweight gnulib test-framework shim:

```bash
# Clone GNU sed testsuite (one time setup)
git clone https://github.com/mirror/sed.git ../gnu.sed

# Run compatibility tests
./util/run-gnu-testsuite.sh

# Verbose mode shows failure details
./util/run-gnu-testsuite.sh -v

# Generate JSON results for CI
./util/run-gnu-testsuite.sh --json-output results.json
```

The harness executes each `.sh` test from the GNU sed testsuite directly, injecting
our Rust sed binary via `PATH` and providing shim implementations of the gnulib test
framework functions (`compare_`, `returns_`, `skip_`, etc.).

### Unit Tests

```bash
cargo test
```

## Extensions and incompatibilities
### Supported GNU extensions
* Command-line arguments can be specified in long (`--`) form.
* Spaces can precede a regular expression modifier.
* `I` can be used in as a synonym for the `i` (case insensitive) substitution
  flag.
* `M` and `m` substitution flags allow multi-line matching.
* In addition to `\n`, other escape sequences (octal, hex, C) are supported
  in the strings of the `y` command.
  Under POSIX these yield undefined behavior.
* The `a`, `c`, and `i` commands do not require an initial backslash,
  allow text to appear on the same line, and support escape sequences
  in the specified text.
* The `a`, `i`, `=`, `l`, `q` and `r` commands support address range as an extension to POSIX.
* The substitution command replacement group `\0` is a synonym for &.
* An `F` command outputs the name of the file currently being processed.
* A `Q` command (optionally followed by an exit code) quits immediately.
* The `q` command can be optionally followed by an exit code.
* A `W` command writes to a file the pattern's first line.
* An `R` command queues a line of a file for the end of the cycle, a further
  line each time it runs.
* `I` and `M` flags on an address regular expression.
* The `l` command can be optionally followed by the output width.
* The `--follow-symlinks` option for in-place editing.
* The `--sandbox` option that limits potentially destructive commands.
* Address 0 can be used to specify an address range that is already
  active on line 1 and can finish with the specified regular expression.
* Address steps can be specified in the form of start~step and start,~step
  ranges.
* Address 0 can be used in the `r` command to prepend a file.
* The special file names of GNU _sed_: `/dev/stdin` for `r` and `R` reads standard
  input, and `/dev/stdout` and `/dev/stderr` for `w`, `W` and the `w` flag of `s`
  write to standard output and error, except in `--posix` mode, as in GNU _sed_.
  `/dev/null` is Windows' `NUL`.

### Supported BSD and GNU extensions
* The second address in a range can be specified as a relative address with +N.
* In-place editing of file with the `-i` flag.

### Incompatible extensions
The `-U` or `--uutil-extensions` option enables useful extensions or bug fixes
that aren't compatible with GNU sed or POSIX.

* The `l` command lists Unicode characters using the `\uXXXX` and `\UXXXXXXXX`
  escapes rather than as octal UTF-8 byte sequences.

### Incompatibilities
* Similarly to GNU _sed_, input is processed as raw bytes or as valid UTF-8
  (this includes 7-bit ASCII) based on the locale as specified by the
  `LC_ALL`, `LC_CTYPE`, and `LANG` environment variables.
  In cash the default, with none of them set, is UTF-8, as the console and awk are;
  GNU _sed_'s is byte processing.
  Any locale that does not name UTF-8 (C, POSIX, `en_US`, `de_DE.CP1252`) is byte
  processing: in contrast with GNU _sed_, the character set it names (e.g.
  ISO-8859-1) is not used. If the input is in another code page or encoding
  and requires locale-specific processing (e.g. ignore/map case,
  character classes), consider converting it through UTF-8 to ensure
  the correct handling of locale-specific regular expressions.
  In UTF-8 mode the character classes (`[[:alpha:]]`, `\w`) follow Unicode's
  properties, which differ from GNU _sed_'s C library in a few characters.
* The word boundaries `\b`, `\B`, `\<` and `\>` take the RE engine's word
  characters, which include combining marks; GNU _sed_'s are letters, digits and
  `_`. A regular expression with back-references, which the engine matches as text,
  reads bytes above 0x7F as Latin-1 letters in byte mode (`LC_ALL=C`) for its word
  boundaries and the `I` flag, where GNU _sed_'s C locale has no letters there.
* The last line (`$`) looks ahead past the input files that are empty or cannot be
  read, and into standard input, as GNU _sed_ does; another input that is not a
  regular file, such as a named pipe, is taken to have lines, where GNU _sed_ reads
  ahead into it.
* In the text of `a`, `i` and `c`, `\c\\` is the control character of a backslash,
  where GNU _sed_ also writes the second backslash.

## GNU test suite compatibility

Below is the evolution of how many GNU tests uutils passes.

![Evolution over time](https://github.com/uutils/sed-tracking/blob/main/gnu-results.svg?raw=true)


## License

sed is licensed under the MIT License - see the `LICENSE` file for details
