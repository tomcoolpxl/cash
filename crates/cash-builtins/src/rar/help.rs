//! The banner and `-?`, in cash's own words: what each command and switch does, not
//! RAR's text.

use std::fmt::Write as _;

use super::Tool;

/// What `-iver` prints: the RAR version whose commands these are, and cash's.
pub(super) const VERSION: &str = "7.23 x64 (cash)";

/// The two lines before a command's output, as rar's banner stands there.
pub(super) const fn banner(tool: Tool) -> &'static str {
    match tool {
        Tool::Rar => {
            "\nRAR (cash)   RAR 7.23's commands and switches, in pure Rust\n\
             cash's own code, not RARLAB's   Type 'rar -?' for help\n"
        }
        Tool::Unrar => "\nUNRAR (cash)   UnRAR 7.23's commands and switches, in pure Rust\n",
    }
}

/// `-?`: how to call the tool, its commands and its switches.
pub(super) fn usage(tool: Tool) -> String {
    let name = match tool {
        Tool::Rar => "rar",
        Tool::Unrar => "unrar",
    };
    let mut text =
        format!("\nUsage:     {name} <command> -<switch 1> -<switch N> <archive> <files...>\n");
    text.push_str("               <@listfiles...> <path_to_extract/>\n\n<Commands>\n");
    for (command, what, in_unrar) in COMMANDS {
        if tool == Tool::Rar || *in_unrar {
            let _ = writeln!(text, "  {command:<14}{what}");
        }
    }
    text.push_str("\n<Switches>\n");
    for (switch, what, in_unrar) in SWITCHES {
        if tool == Tool::Rar || *in_unrar {
            let _ = writeln!(text, "  {switch:<14}{what}");
        }
    }
    text
}

/// Each command: its letters, what it does, whether unrar has it.
const COMMANDS: &[(&str, &str, bool)] = &[
    (
        "a",
        "Put files into an archive, making it if need be",
        false,
    ),
    ("c", "Give the archive a comment", false),
    ("ch", "Change the archive with the switches given", false),
    ("cw", "Write the archive's comment to a file, or out", false),
    ("d", "Take files out of the archive", false),
    ("e", "Extract files, all into one folder", true),
    ("f", "Replace archived files with newer copies", false),
    ("i[par]=<str>", "Look for text inside archived files", false),
    ("k", "Lock the archive against rar's changes", false),
    (
        "l[t[a],b]",
        "List the files: in columns, technically, bare",
        true,
    ),
    (
        "m[f]",
        "Like a, then delete what was put in [files only]",
        false,
    ),
    ("p", "Print files to standard output", true),
    ("r", "Repair a damaged archive", false),
    ("rc", "Rebuild missing volumes from .rev files", false),
    ("rn", "Rename archived files", false),
    ("rr[N]", "Add a recovery record of N percent", false),
    ("rv[N]", "Make recovery volumes (not in cash)", false),
    (
        "s[name|-]",
        "Make or unmake a self-extracting archive (not in cash)",
        false,
    ),
    ("t", "Test the archived files", true),
    ("u", "Add new files and replace older ones", false),
    (
        "v[t[a],b]",
        "List verbosely: sizes, ratios, checksums",
        true,
    ),
    ("x", "Extract files with their folders", true),
];

/// Each switch: its spelling, what it does, whether unrar has it.
const SWITCHES: &[(&str, &str, bool)] = &[
    ("-", "No switches after this", true),
    ("@[+]", "Read @names as names, or [+] as list files", true),
    ("ac", "Clear the archive attribute of files done", false),
    ("ad[1,2]", "A folder per archive when extracting", true),
    ("ag[format]", "Add the date to the archive's name", true),
    ("ai", "Leave attributes alone", true),
    (
        "am[s,r]",
        "Keep, or give back, the archive's name and time",
        false,
    ),
    ("ao", "Only files with the archive attribute", false),
    ("ap<path>", "The folder inside the archive", true),
    (
        "as",
        "Take out of the archive what is not being added",
        false,
    ),
    ("c-", "Do not show comments", true),
    ("cfg-", "Ignore rar.ini and RARINISWITCHES", true),
    ("cl", "Names in lower case", true),
    ("cu", "Names in upper case", true),
    ("df", "Delete files once archived", false),
    ("dh", "Open files other programs are writing", true),
    (
        "dr",
        "Delete files into the recycle bin once archived",
        false,
    ),
    ("ds", "Keep the given order in a solid archive", false),
    ("dw", "Overwrite, then delete, files once archived", false),
    (
        "e[+]<attr>",
        "Leave out [only take] files with these attributes",
        true,
    ),
    ("ed", "No entries for empty folders", false),
    ("ep", "No folders in names", true),
    ("ep1", "No base folder in names", true),
    ("ep2", "Full paths, without the drive", false),
    ("ep3", "Full paths, with the drive", true),
    (
        "ep4<path>",
        "Strip this folder from the front of names",
        true,
    ),
    ("f", "Only files already there, when newer", true),
    ("hp[password]", "Encrypt headers and data", false),
    ("ht[b|c]", "Checksum files with BLAKE2 or CRC32", false),
    (
        "id[c,d,n,p,q]",
        "No banner, Done, names, percentages, or all but errors",
        true,
    ),
    ("ierr", "Everything to standard error", true),
    ("ilog[name]", "Write errors to a log file", true),
    ("inul", "No messages at all", true),
    ("iver", "Show the version", true),
    ("k", "Lock the archive", false),
    ("kb", "Keep files that fail their checksum", true),
    (
        "log[f][=name]",
        "Write archive or file names to a file",
        true,
    ),
    (
        "m<0..5>",
        "How hard to compress: 0 stores, 3 is the default",
        false,
    ),
    ("ma5", "Write RAR 5 (the only format written)", false),
    ("mc<par>", "Fine compression settings", false),
    ("md[x]<n>[kmg]", "The dictionary's size", true),
    (
        "me[par]",
        "Encryption settings: s skips encrypted files",
        true,
    ),
    ("ms[ext;ext]", "Store files of these types", false),
    ("mt<threads>", "How many threads", true),
    ("n<file>", "Only names matching this too", true),
    ("n@<list>", "Only names matching a list file's too", true),
    ("o[+|-]", "Overwrite: ask, always [+], never [-]", true),
    ("oh", "Hard links as links", false),
    ("oi[0-4][:min]", "Identical files as references", false),
    (
        "ol[a,-]",
        "Symbolic links as links [unchecked, left out]",
        true,
    ),
    ("op<path>", "The folder to extract into", true),
    ("or", "Rename files that are there", true),
    ("os", "NTFS streams (not in cash)", true),
    ("ow", "Owners and permissions (not in cash)", true),
    ("p[password]", "The password", true),
    ("qo[-|+]", "Quick open records [none, all]", false),
    ("r", "Into folders", true),
    ("r-", "Never into folders", true),
    ("r0", "Into folders for masks only", false),
    ("rr[N]", "A recovery record", false),
    ("s[=<par>]", "Solid", false),
    (
        "sc<chr>[obj]",
        "The character set of list and comment files",
        true,
    ),
    ("si[name]", "Read data from standard input", true),
    ("sl<size>[u]", "Only files smaller than this", true),
    ("sm<size>[u]", "Only files larger than this", true),
    ("t", "Test the archive once made", false),
    ("ta[mcao]<d>", "Only files changed after a date", true),
    ("tb[mcao]<d>", "Only files changed before a date", true),
    ("tk[<date>]", "Keep the archive's time, or set it", false),
    ("tl", "The archive's time is its newest file's", false),
    ("tn[mcao]<t>", "Only files newer than a while", true),
    ("to[mcao]<t>", "Only files older than a while", true),
    ("ts[m,c,a,p]", "Which times to keep or set", true),
    ("u", "Only new files, and newer ones", true),
    ("v", "List every volume", true),
    ("v<size>[u]", "Volumes of this size", false),
    ("ver[n]", "Keep earlier versions of files", true),
    ("w<path>", "The folder for temporary files", false),
    ("x<file>", "Leave out this file", true),
    ("x@<list>", "Leave out a list file's names", true),
    ("y", "Yes to every question", true),
    ("z[file]", "The archive's comment from a file", false),
];
