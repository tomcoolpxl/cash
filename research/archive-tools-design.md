# Archive and compression tools: one design

Status: **decided by the user on 2026-10-07** (section 7), nothing built yet; TODO.md
phases 26 to 30 and spec D78. It replaced a tar-only plan with one design for the whole
family, as the user asked: "try to make a common thing … think about this properly."

## 1. The family

Every tool below reads or writes the same few things: compressed streams, archive
members, files on a Windows disk. Built one by one, each would grow its own copy of the
same code. `gzip` already shows it: of its 2,357 lines, the option parser, the in-place
file replacement and the message plumbing are not gzip's at all.

| Tool | Interface copied | On Windows today | Git for Windows | Oracle in WSL |
| --- | --- | --- | --- | --- |
| `gzip` `gunzip` `zcat` | GNU gzip 1.14 | cash's own (phase 24) | gzip 1.14 | gzip 1.14 |
| `bzip2` `bunzip2` `bzcat` | bzip2 1.0.8 | nothing (`tar.exe` reads a `.tar.bz2` only) | 1.0.8 | 1.0.8 |
| `xz` `unxz` `xzcat` `lzma` `unlzma` `lzcat` | XZ Utils 5.8 | nothing | none | 5.8.3 |
| `zstd` `unzstd` `zstdcat` | zstd 1.5.7 | nothing | none | 1.5.7 |
| `tar` | GNU tar 1.35 | `tar.exe` = bsdtar 3.8.8 | GNU tar 1.35 | GNU tar 1.35 |
| `zip` `unzip` `zipinfo` | Info-ZIP zip 3.0, UnZip 6.00 | nothing (`tar.exe` reads zips; PowerShell's `Expand-Archive`) | `unzip` only | all three |
| `zgrep` `zdiff` `zcmp` `zless` `zmore` (and `bz*`, `xz*`, `zstd*`) | gzip's, bzip2's, xz's scripts | nothing | gzip's | yes |
| `cpio` | GNU cpio 2.15 | nothing | none | yes |
| `lzip` | lzip 1.24 | nothing | none | none |

## 2. What cash already has, and what it lacks

From a survey of the code on 2026-10-07 (file references as of `17d79882`).

| Need | Today | Verdict |
| --- | --- | --- |
| GNU option parsing (`getopt_long`: clusters, `--name=value`, unique prefixes, ambiguity) | **eleven private copies** of one shape in cash-builtins: gzip, grep, column, hexdump, ss, uuidgen, flock, iconv, watch, free, nc; the `getopt` builtin's own, the most complete (optional values, three orders, long-only); and a clean table-driven `cash-diffutils::getopt`, which cash-builtins cannot reach | one shared parser |
| Replacing a file in place (temporary name beside it, rename, times and read-only kept) | three copies: gzip's `Target`, `dos2unix`'s `write_replacing`, sed `-i` | one helper |
| A file's Unix face: mode bits, owner and group, ids, times, link count, identity | `ls` and `stat` each compute a mode, and **disagree** (`ls` asks the access list for `w`; `stat` looks at read-only and the extension); no group is ever read, `ls` shows the owner twice; `id` maps the process to its RID (0 when elevated); `cash-win32::fs` has owner, RID, access check, link count, file identity | one place, which `ls`, `stat` and the archives share |
| Applying one `s/re/repl/flags` (`tar --transform`) | cash-sed compiles it (`compile`), but the input type has no public constructor, the apply loop is private, and tar's own flags (`r` `s` `h` `x` and capitals) are refused | a small public API in cash-sed |
| Name patterns with GNU's flags | cash-core's `Pattern` (Bash's matching) has case folding, but `*` always crosses `/`, and there is no leading-folder rule; grep and diff use the `glob` crate instead | flags added to `Pattern` |
| Making links, setting times and attributes, checking names | none in cash-win32: no symbolic-link maker, no hard-link wrapper (the binary crate has one), no time or attribute setter beyond std, reserved names (`CON` …) known but used only in tests, nothing for `<>:"|?*` or a trailing dot or space | added to cash-win32 |
| A buffered pull reader for streaming codecs | gzip's `Input` | moves with gzip's codec |
| Registration, help, doctor | the gzip trio is one module with three names (`command!`); pages are found by `build.rs`; `builtins.md`, `CARRIED`, `DELIBERATE_SHADOWS` are hand lists; `cash --link-tools` links every new builtin by itself; `help tools` has GnuWin32 rows for `zip` and `unzip`, to go when cash carries them | the same for every new tool |

Crates today: `cash-win32` and `cash-parser` are leaves; the tool libraries (`cash-sed`,
`cash-awk`, `cash-bc`, `cash-diffutils`) depend on no other cash crate; `cash-picker`
depends on `cash-win32` alone; cash-builtins uses `cash-sed` and `cash-picker` in
process.

## 3. Layers

```
cash-builtins: front ends; each tool's options, words and exit codes, the shell's streams
┌───────────────────────┬──────────────┬─────────────────────┬────────────────────┐
│ compressors            │ tar          │ zip unzip zipinfo   │ z-tools            │
│ one driver,            │ GNU tar 1.35 │ Info-ZIP            │ a codec in front   │
│ a profile per tool     │              │                     │ of grep, diff, less│
└──────────┬────────────┴──────┬───────┴─────────┬───────────┴─────────┬──────────┘
           │                   │                 │                     │
cash-getopt: getopt_long once, with GNU tar's old-style keys; for every GNU-style tool
cash-archive: a library; knows no shell, prints nothing
  codec    gzip bzip2 xz lzma lzip zstd: recognise, decode, encode, describe
  member   an archive entry: name, kind, mode, times, owner, link target
  walk     files on disk → members (operands, -C, recursion, excludes, hard links)
  names    member names ↔ Windows paths, both ways, safely
  extract  members → files (overwrite rules, links, times, read-only)
  select   which members an operand or a pattern names
  listing  mode strings, dates, quoted names
  formats  tar · zip · cpio, each reading and writing members
cash-win32: the Unix face of a Windows file; replace in place; links, times, attributes,
            names Windows cannot hold
cash-core: Pattern with path flags        cash-sed: one s/// compiled and applied
```

**The rule that does the most work: libraries return facts and typed problems; a front
end turns them into its tool's words.** GNU tar says `tar: x: Cannot open: Permission
denied`, UnZip says `error:  cannot create x`; both receive the same
`Problem::Open { name, error }`.

### 3.1 `cash-getopt` (new leaf crate)

`cash-diffutils::getopt` lifted out and grown to what the `getopt` builtin can do:

```rust
pub enum Arg { No, Required, Optional }
pub struct Short { pub letter: char, pub arg: Arg, pub id: &'static str }
pub struct Long  { pub name: &'static str, pub arg: Arg, pub id: &'static str }
pub enum Order { Permute, StopAtOperand, InPlace }
pub enum Item { Option { id: &'static str, value: Option<OsString> }, Operand(OsString) }
pub enum Problem { Unknown(String), Ambiguous { given: String, candidates: Vec<&'static str> },
                   MissingValue(String), UnwantedValue(String) }
pub fn parse(args: &[OsString], shorts: &[Short], longs: &[Long], order: Order)
    -> Result<Vec<Item>, Problem>;
pub fn tar_old_style(args: &[OsString], shorts: &[Short]) -> Vec<OsString>; // tar xzf a.tgz
```

Items come back in order, so `tar -C dir file` keeps its meaning; problems are typed, so
each tool words them (`gzip:` with its backtick hint, `tar:` with GNU tar's). The
diffutils copy and the new tools use it at once; the eleven copies move over one by one
later, each under its own oracle (a TODO item, not this phase).

### 3.2 `codec` (in cash-archive)

```rust
pub enum Codec { Gzip, Bzip2, Xz, Lzma, Lzip, Zstd }
impl Codec {
    pub fn sniff(first: &[u8]) -> Option<Codec>;                    // magic bytes
    pub fn by_suffix(name: &str) -> Option<(Codec, &'static str)>;  // .tgz → (Gzip, ".tar")
    pub fn levels(self) -> RangeInclusive<u32>;  pub fn default_level(self) -> u32;
}
pub fn decoder(codec: Codec, from: Box<dyn Read>) -> Box<dyn Read>;      // concatenated streams too
pub fn encoder(codec: Codec, to: Box<dyn Write>, level: u32) -> Box<dyn Finish>;
pub fn describe(codec: Codec, from: impl Read) -> Result<StreamInfo, CodecError>; // -l
pub enum CodecError { Format, Checksum, Truncated, Unsupported(&'static str), Io(io::Error) }
```

Backends, all Rust: `flate2` (deflate), `bzip2` on `libbz2-rs-sys`, `lzma-rust2` (xz,
lzma, lzip), `ruzstd` (zstd; writes at its fast level only, until `libzstd-rs-sys` has a
Rust API). The `zip` crate is built on the same four, so zip members come out of the
same code. gzip's member reader and writer (header, trailer, the `-N` name and time,
multiple members, `-l`'s numbers) and its `Input` reader move here as `codec::gzip`.

### 3.3 `member`

```rust
pub struct Member {
    pub name: MemberName,            // bytes, '/'-separated, as the archive has them
    pub kind: Kind,                  // File, Dir, Symlink, HardLink, Char, Block, Fifo
    pub size: u64,
    pub mode: u32,                   // Unix bits
    pub mtime: Timestamp,            // seconds and nanoseconds; atime, ctime when known
    pub owner: Owner,                // uid, gid, user name, group name
    pub link: Option<MemberName>,
    pub device: Option<(u32, u32)>,
    pub windows: Option<Attributes>, // read-only, hidden, system: zip keeps them
}
```

tar, zip and cpio read into it and write from it; `walk` makes it; `extract` and
`listing` take it. What only one format has (a tar member's PAX records, a zip entry's
method and comment) travels beside it.

### 3.4 `walk`

Operands in order, with `-C`-style folder changes as steps; recursion; an exclusion
predicate the front end supplies (`--exclude*`, zip's `-x`); dereferencing rules; hard
links found by file identity (`cash-win32::fs::file_info`), so a second name is stored as
a link; sort orders; "changed as we read it" by size and time before and after. Mode,
owner and times from cash-win32's Unix face (3.8), the same that `ls` and `stat` show.

### 3.5 `names`

Storing: `\` to `/`; a drive letter or a leading `/` removed in the words GNU's DOS
builds use (`Removing leading 'C:/' from member names`); `..` as the tool says.
Extracting: `..` that would leave the target refused; absolute names made relative;
names Windows cannot hold reported, never silently changed (3.8); long paths through
`\\?\`; `--strip-components`; a rename hook the front end fills (`--transform` with
cash-sed, 3.10).

### 3.6 `extract`

One safe writer: folders, files streamed from the member, hard links to what was already
extracted, symbolic links when Windows allows them (Developer Mode or an elevated shell)
and a typed refusal otherwise; an overwrite policy (keep, skip, keep newer, overwrite,
unlink first, ask); times; read-only; folder times set after their contents. It never
writes through a link that leads out of the target. `tar -O` and `unzip -p` are the same
writer aimed at standard output.

### 3.7 `select`, `listing`, `formats`

- **select**: an operand or pattern against member names with the flags the tools
  differ on: literal or wildcards, anchored, case, whether `*` crosses `/`, a folder
  naming what is under it (tried by `select` itself, folder by folder). The matching is
  a trait the front end fills with cash-core's `Pattern` (3.9), so cash-archive needs
  no shell crate.
- **listing**: mode strings, dates in the shell's `TZ` (the front end passes the zone),
  sizes, name quoting (GNU's escape style, UnZip's raw).
- **tar**: its own block loop, for GNU's behaviour on damaged archives (`Skipping to
  next header`, a lone zero block, a bad checksum), and its own headers, read and
  written (see 3.12 for why not the `tar` crate's). Writing builds the 512-byte headers
  here, so a deterministic archive is GNU's byte for byte (the checksum field's format,
  padding to the 10,240-byte record).
- **zip**: its own records, read and written (see 3.13 for why not the `zip` crate's),
  with Info-ZIP's extra fields (extended times, Unix owners), so archives round-trip with
  Info-ZIP, Explorer and `tar.exe`.
- **cpio**: newc and odc, small enough to write here.

### 3.8 cash-win32: one Unix face for a Windows file

```rust
pub struct UnixView { pub mode: u32, pub owner: Account, pub group: Account,
                      pub links: u32, pub times: Times, pub identity: FileInfo }
pub struct Account { pub name: String, pub id: u32 }        // id: the RID, as `id` and `stat` use it
pub fn unix_view(path: &Path, follow: bool) -> io::Result<UnixView>;
pub fn replace(target: &Path) -> io::Result<Replacement>;   // gzip's Target, for everyone
pub fn symlink(target: &Path, link: &Path, kind: LinkKind) -> io::Result<()>;
pub fn hard_link(existing: &Path, new: &Path) -> io::Result<()>;
pub fn set_times(path: &Path, times: &Times) -> io::Result<()>;   // folders too
pub fn set_attributes(path: &Path, attributes: Attributes) -> io::Result<()>;
pub fn check_name(component: &OsStr) -> Result<(), NameProblem>; // reserved, <>:"|?*, trailing . or space
```

`ls`'s rule for the mode (the access list decides `w`) is the one kept; `stat` moves to
it, which changes what `stat` prints for some files, and the group is read for the first
time instead of repeating the owner.

### 3.9 cash-core: `Pattern` with path flags

`Pattern` gains `set_pathname` (`*` and `?` do not cross `/`) and `set_period` (a
leading dot only by name), the two FNM flags GNU tar and UnZip need; Bash's own matching
is unchanged, since both default off.

### 3.10 cash-sed: one `s///`

```rust
pub fn compile_substitution(expr: &[u8], extra_flags: &dyn Fn(u8) -> bool)
    -> Result<Substitution, String>;
pub fn substitute(s: &Substitution, input: &[u8]) -> Option<Vec<u8>>;
```

`extra_flags` lets tar accept its `r` `s` `h` `x` flags; nothing about `sed` itself
changes.

### 3.11 The compressors (cash-builtins): shared blocks, a flow per tool

`gzip`, `bzip2`, `xz`, `zstd` and their aliases look alike from afar: compress or
decompress files in place by suffix, `-c`, `-d`, `-k`, `-f`, `-t`, `-q`, `-v`, levels,
standard input and output, a refusal to write compressed data to a terminal. The plan
was one driver with a `Profile` per tool. Built against the four oracles (phase 28,
2026-10-07), that did not hold: the tools check a file in different orders (bzip2: the
input exists, its suffix, a directory, the output exists, its links; xz: it opens the
input, reads the first 8 KiB to tell the format, and only then looks at the suffix and
the output; zstd: the suffix before the file exists), read their command lines
differently (bzip2 two passes over flags placed anywhere, zstd its own loop with
prefix-matched long options and values glued to letters, gzip and xz `getopt_long`),
answer an existing output differently (gzip and zstd ask, bzip2 and xz refuse), keep
or remove the input by default differently, buffer standard output differently (bzip2's
and zstd's stdio buffer puts an error before the data; gzip and xz write as they go),
and word every failure in their own long or short way. A profile would have been all
exceptions.

So the shared parts are building blocks, and each tool keeps its own flow, written
after its original's functions (bzip2's `compress`/`uncompress`/`testf`, xz's
`coder_run` with `io_open_src`/`io_open_dest`, zstd's `FIO_*` and its main loop):

- `cash-builtins/src/compress.rs`: where a file's output goes (`Output`: standard
  output, nowhere for `-t`, or a `Replacement` file), byte counts for `-v` (`Counted`),
  the console's yes or no (`answer_is_yes`), `is_terminal`, `strerror`, `times_of`.
- cash-archive's codecs, each with what its tool needs beyond a reader and a writer:
  `codec::bzip2::decompress_streams` (one stream at a time, "no stream here" told from
  damage), `codec::xz` (xz's own format tests, liblzma's view of what may follow a
  stream, the presets with `-e`, the .xz index for `-l`), `codec::zstd`
  (`decompress_frame` with zstd's error words and its 128 KiB output buffer, frames that
  say their content size), `codec::Lookahead` (look at the next bytes before reading).
- cash-win32's `Replacement` (written beside the target, renamed over it) and the Unix
  face (read-only and times carried over).
- tests/it/common.rs: the oracle helpers, with divergences stated where cash differs.

### 3.12 tar, as built (phase 29, 2026-10-07)

Built against GNU tar 1.35's oracle, the plan moved in four places:

- **The headers are cash's own** (`cash-archive/src/tar`: `header`, `read`, `write`),
  not the `tar` crate's. Byte-for-byte archives need GNU's field formats (`%0*o` with a
  NUL, the checksum's `%06o`, a NUL and a space, base-256 past the octal range, the
  `././@LongLink` headers with their own mode, owner and time, the ustar prefix split,
  no device numbers for a file, the type bits in the old GNU mode). Reading needed the
  block loop anyway, and the crate's PAX parser is twenty lines. Nothing of the crate
  was left to use. Sparse members are read whole in GNU's four forms (the old `S` header
  with its extension blocks, pax 0.0, 0.1 and 1.0); none are written, as Windows'
  sparse files are rare: `-S` stores holes as zeros, as GNU does without it.
- **`walk`, `names` and `extract` live in the tar front end** (`cash-builtins/src/tar`:
  `create`, `list`), not in cash-archive. GNU tar's walk and extraction say things
  between their steps: the `-v` line before a tag's warning, "Removing leading" once
  for each prefix, a hard link known on first sight, "file is the archive". A library
  for them would have one user and an interface shaped by tar's messages. Phase 30 moves
  into cash-archive what zip shares with them, when there are two users to shape it.
- **`select` has its own `fnmatch`** (glibc's, with GNU tar's leading-folder, case and
  unanchored retry), not cash-core's `Pattern` with new flags (3.9 is not done): the
  retry after each `/` and the leading-folder rule live in the same function as the
  wildcards.
- **`--transform`** (`tar/transform.rs`) reads its regular expressions through
  cash-sed's translation of GNU's syntax, with tar's own `s///` parser and flags (`r`,
  `s`, `h`, `x`, `g`, a number), not a new cash-sed entry point (3.10 is not done).

### 3.13 zip, unzip and zipinfo, as built (phase 30, 2026-10-07)

- **The records are cash's own** (`cash-archive/src/zip`: `read`, `write`, `crypt`), not
  the `zip` crate's. zipinfo's `-v` prints every field of every record (the version
  made by and needed, the flags, the internal attributes, each extra field), and zip's
  own records carry what the crate does not let a writer set: "made by Unix, 3.0", the
  deflate level in the flags, the text bit, the descriptor and encryption choices. A
  stored archive without extra fields (`-0 -X`) is Info-ZIP's byte for byte, which the
  oracle checks. The codecs are the ones the other tools use (deflate on `flate2`,
  bzip2, LZMA and xz on `lzma-rust2`, zstd on `ruzstd`), and Deflate64 on the small
  `deflate64` crate; the traditional encryption is forty lines.
- **The front ends follow Info-ZIP's own flows**: UnZip's `extract_or_test_member` and
  its messages, ZipInfo's listings, zip's `zipup` with the retry as stored, its update
  order (members replaced where they stand, new ones after), and its option table.
  `select`'s `fnmatch` serves both (UnZip's `*` crosses `/` unless `-W`).
- **Decided in the phase**, under the instruction to finish phase 30 without stopping,
  and open to the user's change: deflate on `miniz_oxide` (gzip's already; output
  differs from Info-ZIP's own deflate either way); the numeric owner is the RID, as
  `id`, `stat` and tar give it; a zip says it was made on Unix (3.0), so Linux's UnZip
  restores the modes from the Unix face, and MS-DOS's directory and read-only bits are
  set beside them for Windows' readers.
- **Beyond Info-ZIP's unzip**, added on the user's word the same day: `WinZip`'s AES on
  RustCrypto's `aes`, `hmac`, `pbkdf2` and `sha1`; PPMd on `ppmd-rust`; PKZIP 1's
  shrunk, reduced and imploded members through the `zip` crate's own decoders, its only
  use (the crate with its legacy feature alone); split archives read from all their
  parts. zip's `-s` writes split archives byte for byte as zip 3.0 does, and its `-F`
  and `-FF` repair as it does.
- **Not moved into cash-archive**: tar's walk and extraction stay in tar's front end,
  as zip's do in zip's. The two walks share less than 3.4 planned: tar sorts, follows
  `-h`, counts hard links; zip filters by date and suffix and never stores links unless
  `-y`. A shared walker would be two walkers behind one name.

### 3.14 7z and 7za (phase 31, decided 2026-10-07)

7-Zip 26.03's command line, under both names, on the formats cash already has and on
7z itself. The user's picks are in section 7; what follows is the plan they set.

- **The 7z format is cash's own, from `sevenz-rust2`** 0.23.0 (Apache-2.0), taken into
  `cash-archive/src/sevenz` as source, "our own version from here", neither a
  dependency nor a vendored copy with patches. Its license goes to `licenses/` and
  `NOTICE`, and each file it came from says so. What changes:
  - packed blocks copied raw, so `d`, `u` and `rn` leave the blocks they do not touch,
    and their encryption, as they were (7-Zip does; the crate can only recompress);
  - the facts 7-Zip lists: physical and headers size, the coder chain by 7-Zip's names
    (`LZMA2:24 BCJ`, `7zAES:19`), encrypted headers, entries in archive order, raw NT
    times, attributes with the Unix-mode extension read;
  - 7-Zip's `-mx` table (dictionary, word and solid block sizes), `-mhc`, `-mhe`, `-ms`,
    `-mf`, `-mtm`/`-mtc`/`-mta`, AES with 2^19 rounds; the LZMA2 dictionary property's
    rounding fixed; one thread unless `-mmt`;
  - typed errors: a wrong password told from a CRC failure, truncated data noticed, no
    panic on a header without file records or a time past 2^63;
  - the codecs shared with the rest of cash-archive (deflate on `flate2`, Deflate64, zstd
    on `ruzstd` read); brotli, lz4 and the wasm parts left out.
- **Formats**: 7z read and written; zip, tar, gzip, bzip2 and xz read and written
  through cash-archive; zstd and lzma read, as 7-Zip has them. The type comes from `-t`,
  then the archive's first bytes, then the name's suffix when creating; any other
  (rar, iso, cab, wim …) gets 7-Zip's "Cannot open the file as archive". A zip that 7z
  writes is 7-Zip's (made on Windows, its NTFS time field), not zip's Info-ZIP records.
- **Face**: Windows 7-Zip's words, listings, attributes (no Unix modes stored) and exit
  codes, with LF line ends and `/` in names, as cash's tar and zip print. The banner's
  place holds `7-Zip (cash)` and what it copies, as the other tools' version lines do;
  `-ba` leaves it out. Switches start with `-` only (7-Zip on Windows takes `/` too,
  which in cash is a path).
- **Volumes** (`-v`): `NAME.001`, `NAME.002` … for every type, as 7-Zip's byte splits;
  `NAME.001` read back from all its parts, 7-Zip's Split handler.
- **`h`** and `-scrc`: CRC32, CRC64, XXH64, MD5, SHA-1, SHA-256, SHA-384, SHA-512,
  SHA3-256 and BLAKE2sp, `*` for all, with 7-Zip's tables; on `crc`, `twox-hash`,
  `md-5`, `sha1`, `sha2`, `sha3` (in the tree already) and `blake2s_simd` (new).
- **Links**: `-snl` stores a symbolic link as a link (in 7z, its reparse data with the
  reparse attribute, as 7-Zip on Windows; in tar, a link member); `-snh` stores hard
  links as links in tar, while 7z, which has no record for one, gets the file, as 7-Zip
  does. Links are made on extraction where Windows allows them.
- **NTFS extras**, as 7-Zip does: `-sns` and `-sni` writing 7z, zip or tar end in
  "System ERROR: Not implemented"; extracting writes `file:stream` items as alternate
  streams, unless `-sns-`.
- **Refused**: `b`, `-sfx`, `-seml`, `-slp`, `-stm`, `-ad`, with 7-Zip's words for an
  unsupported switch.
- **Front end** `cash-builtins/src/sevenzip`: 7-Zip's parser (switches anywhere,
  `--`, `@listfile`, `-i`/`-x`/`-ai`/`-ax` with `r`, `m`, `w` and `!`), its update
  matrix (`-u` with `p` `q` `r` `x` `y` `z` `w` and `!newArchive`), the overwrite
  question and `-ao`, `-o`, `-p` (asked at the console when needed), `-r`, `-y`, `-so`,
  `-si`, `-sdel`, `-stl`, `-spf`, `-spe`, `-ssc`, `-w`, `-bb`, `-bs`, `-bd`, `-bt`, the
  progress line on a console, `-slt`.
- **Oracle**: `tests/oracle/7z_cases.sh`, made by Scoop's 7-Zip 26.03 under a cash that
  has no `7z` builtin, CRLF and `\` turned to LF and `/`. Archives made with `-mx0
  -mhc=off` are 7-Zip's byte for byte; compressed sizes are compared apart, since the
  LZMA encoder is not 7-Zip's.

## 4. Rules every part keeps

- **No C.** Every backend is Rust; `cargo deny` and the release's license list stay the
  gate (bzip2-1.0.6 accepted on 2026-10-07).
- **Libraries know no shell**: no current folder (front ends resolve operands with the
  shell's), no terminal, no printing. They take `Read`/`Write` streams, a probe for
  Ctrl-C (the front end passes the shell's pending-interrupt flag, as `watch` and
  `flock` poll it), and an observer for `-v` lines, so they interleave with errors in
  GNU's order.
- **Words belong to front ends**, copied exactly from their tool; the oracles check them.
- **Security**: `..`, absolute names and links out of the target refused on extraction;
  control characters escaped in listings; no size limits, as the originals have none.
- **Windows facts in one place**: modes, owners, times, links, names, long paths, all in
  cash-win32, used by `walk`, `names`, `extract`, and by `ls` and `stat`.
- **Built-ins, in process**, registered like the gzip trio (one module, several names),
  each with `.with_substitution_files()`.

## 5. Tests

- Libraries: round trips for every codec and format; a shared corpus of damaged inputs;
  `cash-getopt` against the `getopt` builtin's oracle.
- Each tool: an oracle script under the original in WSL (`tests/oracle/*_cases.sh`), as
  gzip has, every message and status.
- Across tools: what cash makes, read by the originals (GNU tar, UnZip, Explorer,
  `tar.exe`), and the originals' archives read by cash.
- The moved code keeps its tests: gzip's oracle, `ls`'s and `stat`'s, `dos2unix`'s,
  diff's and cmp's.

## 6. Order of work

1. **The groundwork**: `cash-getopt`; cash-win32's Unix face and helpers, with `ls`,
   `stat` and `dos2unix` moved onto them; cash-archive with `codec`; the compressor
   driver with gzip moved onto it. Nothing new for the user yet, and every move has a
   test that already passes.
2. **`bzip2`, `xz`, `zstd`** with their aliases: a bare `.bz2`, `.xz` or `.zst` has no
   reader on Windows today.
3. **`member`, `names`, `walk`, `extract`, `select`, `listing`, and `tar`**, with
   cash-core's flags and cash-sed's `s///`.
4. **`zip`, `unzip`, `zipinfo`** on the same layers; the GnuWin32 rows leave `help tools`.
5. Not wanted now: the z-tools, `cpio`, `lzip`.

## 7. Decided by the user (2026-10-07, by pick lists)

1. **The family**: the `bzip2`, `xz` and `zstd` commands with their aliases, `tar`, and
   `zip`, `unzip`, `zipinfo`. Not now: the z-tools, `cpio`, `lzip`.
2. **The crates**: `cash-archive` and `cash-getopt`, both new.
3. **Moved onto the shared parts**: gzip; `ls` and `stat`; `dos2unix`; and all eleven
   option parsers, not later but in the groundwork.
4. **The order**: the groundwork (phases 26 and 27), the compressors (28), tar (29),
   zip (30).
5. **7z** (phase 31, for 1.10.0; 3.14): a builtin named `7z` and `7za`, reading and
   writing; 7z and the formats cash already has; Windows 7-Zip's words with LF and `/`;
   `sevenz-rust2` internalized; volumes, `h`, links, and `-sns`/`-sni` as 7-Zip has
   them.

## 8. Open, decided later in their phases

- Decided in phase 30 (3.13): deflate on `miniz_oxide`; the RID as the numeric owner; a
  zip made on Unix. Each is the user's to reopen.
- `zstd` levels above `ruzstd`'s fast one.
