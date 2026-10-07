//! The usage texts: Info-ZIP's, with cash's own first lines.

/// `unzip`'s usage, after its version line.
pub(super) const UNZIP_USAGE: &str = "\
UnZip (cash): UnZip 6.00's options, in pure Rust.

Usage: unzip [-Z] [-opts[modifiers]] file[.zip] [list] [-x xlist] [-d exdir]
  Default action is to extract files in list, except those in xlist, to exdir;
  file[.zip] may be a wildcard.  -Z => ZipInfo mode (\"unzip -Z\" for usage).

  -p  extract files to pipe, no messages     -l  list files (short format)
  -f  freshen existing files, create none    -t  test compressed archive data
  -u  update files, create if necessary      -z  display archive comment only
  -v  list verbosely/show version info       -T  timestamp archive to latest
  -x  exclude files that follow (in xlist)   -d  extract files into exdir
modifiers:
  -n  never overwrite existing files         -q  quiet mode (-qq => quieter)
  -o  overwrite files WITHOUT prompting      -a  auto-convert any text files
  -j  junk paths (do not make directories)   -aa treat ALL files as text
  -U  use escapes for all non-ASCII Unicode  -UU ignore any Unicode fields
  -C  match filenames case-insensitively     -L  make (some) names lowercase
  -X  restore UID/GID info                   -V  retain VMS version numbers
  -K  keep setuid/setgid/tacky permissions   -M  pipe through \"more\" pager
  -O  CHARSET  specify a character encoding for DOS, Windows and OS/2 archives
  -I  CHARSET  specify a character encoding for UNIX and other archives

See \"unzip -hh\" or unzip.txt for more help.  Examples:
  unzip data1 -x joe   => extract all files except joe from zipfile data1.zip
  unzip -p foo | more  => send contents of foo.zip via pipe into program more
  unzip -fo foo ReadMe => quietly replace existing ReadMe if archive file newer
";

/// `unzip -v` without an archive: what this unzip is.
pub(super) const UNZIP_VERSION: &str = "\
UnZip (cash): UnZip 6.00's options, in pure Rust.

Reads stored, deflated, Deflate64, bzip2, LZMA, xz and zstd members, Zip64 archives,
and the traditional PKWARE encryption; restores times and the read-only attribute.

UnZip and ZipInfo environment options:
           UNZIP:  read before the command line
         ZIPINFO:  read before zipinfo's command line
";

/// `zipinfo`'s usage.
pub(super) const ZIPINFO_USAGE: &str = "\
ZipInfo (cash): ZipInfo 3.00's options, in pure Rust.

List name, date/time, attribute, size, compression method, etc., about files
in list (excluding those in xlist) contained in the specified .zip archive(s).
\"file[.zip]\" may be a wildcard name containing *, ?, [] (e.g., \"[a-j]*.zip\").

   usage:  zipinfo [-12smlvChMtTz] file[.zip] [list...] [-x xlist...]
      or:  unzip -Z [-12smlvChMtTz] file[.zip] [list...] [-x xlist...]

main listing-format options:             -s  short Unix \"ls -l\" format (def.)
  -1  filenames ONLY, one per line       -m  medium Unix \"ls -l\" format
  -2  just filenames but allow -h/-t/-z  -l  long Unix \"ls -l\" format
                                         -v  verbose, multi-page format
miscellaneous options:
  -h  print header line       -t  print totals for listed files or for all
  -z  print zipfile comment   -T  print file times in sortable decimal format
  -C  be case-insensitive     -M  page output through built-in \"more\"
  -x  exclude filenames that follow from listing
  -O  CHARSET  specify a character encoding for DOS, Windows and OS/2 archives
  -I  CHARSET  specify a character encoding for UNIX and other archives
";

/// `zip -h`.
pub(super) const ZIP_USAGE: &str = "\
Zip (cash): Zip 3.0's options, in pure Rust. Usage:
zip [-options] [-b path] [-t mmddyyyy] [-n suffixes] [zipfile list] [-xi list]
  The default action is to add or replace zipfile entries from list, which
  can include the special name - to compress standard input.
  If zipfile and list are omitted, zip compresses stdin to stdout.
  -f   freshen: only changed files  -u   update: only changed or new files
  -d   delete entries in zipfile    -m   move into zipfile (delete OS files)
  -r   recurse into directories     -j   junk (don't record) directory names
  -0   store only                   -l   convert LF to CR LF (-ll CR LF to LF)
  -1   compress faster              -9   compress better
  -q   quiet operation              -v   verbose operation/print version info
  -c   add one-line comments        -z   add zipfile comment
  -@   read names from stdin        -o   make zipfile as old as latest entry
  -x   exclude the following names  -i   include only the following names
  -F   fix zipfile (-FF try harder) -D   do not add directory entries
  -A   adjust self-extracting exe   -J   junk zipfile prefix (unzipsfx)
  -T   test zipfile integrity       -X   eXclude eXtra file attributes
  -y   store symbolic links as the link instead of the referenced file
  -e   encrypt                      -n   don't compress these suffixes
  -h2  show more help
  
";

/// `zip -v` alone: what this zip is.
pub(super) const ZIP_VERSION: &str = "\
Zip (cash): Zip 3.0's options, in pure Rust.

Writes stored, deflated (miniz_oxide) and bzip2 members, Zip64 when a member or the
archive needs it, Info-ZIP's universal-time and Unix owner fields, and the traditional
PKWARE encryption.
";
