# Shared parts for the builtins: options, inputs, walks

The question (2026-10-16): cash's own builtins each parse their options, open their
files and walk their directories themselves. Should those be parts they share? Three surveys
of every builtin (option parsing, input opening, directory walks) answered yes for all
three. The user chose all three: **a GNU option parser, an input opener and a directory
walker**. This note comes before any code. It covers what each tool does today, the bugs
that come from doing it per tool, the design of each part, and the order the tools move
over in.

The rule for every move: one tool at a time, each against its tests and Git Bash, and its
messages and statuses stay its upstream's. A shared part takes over the mechanics. What a
tool prints stays the tool's.

The design below is a proposal. The choices still open are listed at the end. What the
user picks goes into `decisions.md`.

## What the surveys found

### Options

- **The parser exists.** `cash-getopt` is glibc's `getopt_long` and is already used by 16
  tools: free, flock, column, gzip, grep, getopt, iconv, hexdump, nc, ss, xz, watch,
  uuidgen, tar, diff and cmp. It does:
  - short clusters and long options with unique-prefix abbreviation, using glibc's
    ambiguity rules;
  - optional values, permutation or stopping at the first operand, `--`, and
    `long_only`;
  - tar's old-style first word;
  - an in-order reader;
  - glibc's exact messages.
- **Hand-written parsers:** names (basename, dirname), mkfifo, rev, locale, stdbuf,
  nice, pkill, killall, pidof, tput and ping. Each gets something wrong:
  - no abbreviations;
  - the wrong letter blamed in a cluster (`mkfifo -Zm`, `rev -0h`);
  - `--mode MODE` refused;
  - `--` unreachable (locale);
  - `=value` dropped from "unrecognized option".
- **clap:** xargs, ls, stat, tree, top, pgrep, ps, chmod, chown and others. They get
  clap's wording and status 2, have no abbreviations, and **refuse a repeated option**.
  Only `type` and `.` set `args_override_self`.
- **Own syntax, by design:** bzip2, zstd, zip, unzip, zipinfo, 7z, rar, xxd, dos2unix,
  lsof, stty, ps, renice and find's expression. These are not getopt-like upstream and
  stay as they are.

### Inputs

- Nearly every tool opens a named file the same way: `absolute_path`, then a
  fully-shared `File::open` (std's default sharing already meets D33), then the error
  worded through `os_error_text`, then the next file.
- **A builtin cannot open `/dev` names.** The hook that serves them is installed only in
  bundled tools' processes, and only tar matches the exact strings.
- **A directory reads as "Permission denied"** wherever no stat comes first. `CreateFileW`
  refuses a directory opened without backup semantics.
- **Process substitution:** a builtin gets a `<(…)` as a named pipe that can be opened
  twice. Tools that stat and then open use both opens, and bzip2's `-f` passthrough opens
  a third time.
- **Stops:** whole-file readers and most standard inputs do not ask the stop token.
- **Duplicates:**
  - five pipe and device checks that disagree on case;
  - two copies of the Windows-message helper;
  - seven ways to word a directory.

### Walks

- 15 walks in the builtins, plus globstar, `rm`, `diff` and the picker outside them.
- Eleven are recursive Rust functions. None opens a directory once.
- **Loops:** only `find -L` detects them. grep -R, gzip -r, zstd -r, zip, rar a and 7z a
  (all three by default), tar -h and diff -r follow links with no loop check.
- **What counts as a link differs:**
  - tree and 7z test the reparse-point attribute, so a OneDrive or dedup directory is a
    "link" to them (tree counts it as a file);
  - every other walk uses std's name-surrogate test (symlinks and junctions).
- **Unreadable directories are silent** in chmod -R (status 0), where -r, zstd -r, zip, rar,
  fileuse, `ls --tree` and globstar.
- **Names:**
  - gzip, zstd, tar, zip, rar and 7z rebuild child paths from lossy strings, so a name
    with a lone surrogate cannot be reached;
  - `ls -R foo/` prints `foo//bar:`.
- **Stops:** ten walks never ask the stop token.

### Checked against Git Bash (2026-10-16)

| Command | Git Bash | cash |
|---|---|---|
| `ls -F -F f`, `ls --color=auto --color=never f` | `f` | clap: "cannot be used multiple times", status 2 |
| `basename foo-bar -bar` | `foo` | invalid option `-b` (cash permutes; GNU's optstring is `+as:z`) |
| `echo x \| grep x /dev/stdin` | `x` | No such file or directory |
| `locale --` | the locale | unrecognized option `--` |
| `mkfifo --mode 644` | missing operand | unrecognized option `--mode` |
| `stat` | `stat: missing operand` + Try | clap's required-argument block |
| `echo a \| xargs -x echo` | `a` | clap: unexpected argument |
| `column somedir` | read failed: Is a directory | Permission denied |

The `ls` case is the one users meet. `alias ls='ls --color=auto'` followed by
`ls --color=never` fails.

The compressors' data loss on a stop, which the walk survey found, is fixed already
(d128924e). The rule it left: a stop must fail a read, never end it.

## The option parser: `cash-getopt`, finished, and the tools on it

cash-getopt needs no redesign. It needs what its users miss:

1. **`POSIXLY_CORRECT`**: an `Options::posix(bool)` that stops at the first operand. The
   tool passes the shell variable. cash-getopt does not depend on cash-core, so diff and
   cmp keep it.
2. **A claim hook** for words that are not options: `-9`, `-KILL` (pkill, killall, fuser,
   kill), `-10` and `--5` (nice), `-NUM` (head-style). It is a callback that sees each
   word starting with `-` before the parser does and may take it. grep's digit options
   keep their current form.
3. **The usage-error line in cash-builtins**:
   - `usage_error(context, tool, problem, Style)`;
   - the styles: GNU's Try line, gzip's and procps' backtick Try, argp's
     "Try … or … --usage", the usage block (procps, BSD), and xz's prefixed Try;
   - each tool names its style once.
4. **Exclusive groups**: one `exclusive(groups, seen)` for uuidgen's and column's copies.
5. **clap builtins**:
   - `args_override_self(true)` in the shared clap path (`cash-core` `builtins.rs`), so a
     repeated option overrides, as in Bash and GNU. This fixes `ls` with an alias at
     once, and every other clap builtin with it.
   - Tools whose upstream is getopt move off clap after that, to get GNU's messages and
     abbreviations: xargs, ls, stat, pgrep.
   - find keeps its expression parser and loses only clap's `-h`/`--help`.

The order for options:
1. the clap override;
2. names (basename stops at the first operand);
3. mkfifo, rev, locale, stdbuf and nice;
4. pkill, killall and pidof (claim hook), and tput and ping;
5. flock, watch and nc act on help and version in order;
6. xargs, ls, stat and pgrep off clap.

## The input opener: `ExecutionContext::open_input`

It lives in cash-core, beside `Shell::open_file`, whose `/dev` handling it shares.
`/dev/tcp` and `/dev/udp` stay for redirections only.

```rust
pub struct Open { pub dash: Dash, pub directories: Directories }
pub enum Dash { Stdin, File }               // `-`: standard input, or a file named `-`
pub enum Directories { Refuse, Allow }          // Refuse: IsADirectory, not "Permission denied"

pub struct Input {
    pub shown: String,                      // the name as the script gave it
    pub source: Source,                     // File { path, metadata } | Stdin | Device | Pipe
    reader: StopReader<Box<dyn Read + Send>>,
}
impl Input {
    pub fn metadata(&self) -> Option<&Metadata>;   // of the open handle, no second open
    pub fn read_all(self) -> io::Result<Vec<u8>>;  // the whole-file readers
}
impl Read for Input { … }                          // fails with `Stopped` at a stop

pub enum InputError { NotFound, PermissionDenied, IsADirectory, Other(io::Error) }
impl InputError { pub fn reason(&self) -> String }  // strerror words, as os_error_text
```

- **One open, then the handle's metadata.** A failed open with PermissionDenied is
  stat'ed once to tell a directory apart. The happy path costs nothing extra, and a
  substitution pipe is opened once.
- **`/dev` names:**
  - null reads as empty;
  - tty opens the console;
  - zero, random and urandom open the endless device;
  - `stdin`, `stdout`, `stderr` and `fd/N` open the shell's own descriptor N.
- **Names resolved with `absolute_path`, always.** This fixes 7z's list files, which
  resolve against the directory cash started in.
- **The reader always goes through the stop token**, standard input included.
- **What stays the tool's own:**
  - the wording of every message and status;
  - gzip's, unzip's and rar's suffix probing (the tool asks for each name it tries);
  - seekable archive input and volumes (zip, 7z, rar, tar's update);
  - rar's deliberately unshared open, and every write, lock or copy open.

The order for inputs:
1. grep (`/dev/stdin`);
2. rev, column, hexdump, iconv, xxd and logger;
3. dos2unix and the pager;
4. the compressors' named files and standard input (bzip2's third open goes);
5. tar's `-T`/`-X`, 7z's list files and rar's list files.

## The walker: on `walkdir`, through `cash_builtins::walk`

walkdir 2.5 is in the tree already. It is the established crate, and it covers almost
every option the survey lists:
- iteration instead of recursion;
- following links, or following only the command line's (`follow_root_links`, for `-H`);
- loop detection by file identity, with the ancestor named in the error;
- `sort_by`, `contents_first` and `min_depth`/`max_depth`;
- `same_file_system`;
- `skip_current_dir` for pruning;
- lossless `PathBuf`s;
- the free metadata of a directory listing.

Writing a walker of cash's own would mean writing all of that again. cash adds a thin
layer:

```rust
pub struct WalkOptions {
    pub links: Links,            // Never | Roots | All
    pub order: Order,            // Listing | Ordinal | Folded | By(fn(&Entry, &Entry) -> Ordering)
    pub contents_first: bool,
    pub depth: RangeInclusive<usize>,
    pub one_file_system: bool,
}
pub struct Entry {
    pub shown: String,           // the root as given, then `/name`s, no doubled `/`
    pub path: PathBuf,           // absolute, lossless
    pub depth: usize,
    pub kind: Kind,              // File | Directory | Link | Other
}
pub enum WalkError {
    Open { shown, error },       // a directory that cannot be listed
    Entry { shown, error },      // an entry that cannot be looked at
    Loop { shown, ancestor },    // a followed link back to a directory above it
}
impl Iterator for Walk { type Item = Result<Entry, WalkError>; }   // `Stopped` at a stop
impl Walk { pub fn skip_directory(&mut self); }                       // prune, as find -prune
```

- **One answer to what a link is:** a symlink or junction (a name surrogate), as std and
  walkdir have it. A cloud placeholder, a dedup file or an AppExecLink is an ordinary
  file or directory. That changes tree and 7z, which today treat every reparse point as a
  link.
- **The stop token** is checked for every entry.
- **Every error goes to the tool** to word, status and continue. None is silent unless
  the tool chooses silence, as upstream does.
- **Shown names** are joined in one place. That fixes `ls -R foo/`.
- **What stays the tool's own:**
  - ls -R's listing per directory (the walker gives the directories, ls lists each);
  - where's and rar open's two passes, files and then directories (a `By` order with files
    first);
  - the archivers' filters, which become prune calls;
  - the picker, which uses the `ignore` crate for `.gitignore`.

The order for walks:
1. grep -r/-R (loops);
2. find (its loop code goes, its message stays);
3. tree;
4. gzip -r and zstd -r (links as GNU gzip and zstd treat them);
5. chmod -R (an unreadable directory stops being silent);
6. where -r (each directory listed once) and fileuse;
7. tar, zip, rar and 7z last, since they have the most options: walking, loops and
   stops come from the walker, archive reading stays theirs.

## Tests

- Each move keeps the tool's own tests passing and adds one that fails without it, where
  a bug goes away.
- The checked cases above become tests against Git Bash's answers, as names.rs's 42 cases
  are.
- The walker gets a unit test on a temporary tree with:
  - a junction loop, a symlink to a file and a broken link;
  - an unreadable directory;
  - a name with a lone surrogate;
  - each order.
- The opener gets one for:
  - each `/dev` name;
  - a directory;
  - a substitution pipe opened once;
  - a stop in the middle of a read.

## Open choices

1. **The walker's basis.**
   - Proposed: walkdir, already in the tree, with cash's layer on top.
   - The alternative: a walker of cash's own. It gives full control (opening each directory
     once, handle-relative reads) at the cost of writing loop detection, link following
     and ordering again.
2. **Which part first.**
   - Proposed: options, inputs, then walks. The parser exists, so the `ls` alias failure
     and basename land in the first commits.
   - Walks first would end the link loops in eight tools sooner.
   - Inputs first would fix `/dev/stdin` and directory messages sooner.
3. **The archivers.**
   - Proposed: tar, zip, rar and 7z move onto the walker last. Their link loops and stops
     come from it, and their own options become prune calls.
   - The alternative: each gets a loop check and stop checks in its own walk, and keeps
     the walk.
4. **The clap builtins.**
   - Proposed: repeated options override, everywhere, at once. Then xargs, ls, stat and
     pgrep move to cash-getopt, for GNU's messages and abbreviations (`ls --al`).
   - The alternative: only the override; clap's messages stay.
