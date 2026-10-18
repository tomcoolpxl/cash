//! The tools cash took from uutils and made its own (in `crates/cash-uutils`) say their
//! messages in English, and in GNU's words. A release printed their message ids instead
//! (`sort: sort-cannot-read`, `Usage: tee-usage`): uucore finds a tool's messages in the
//! cargo registry, where a tool that is not a crates.io crate is not, on a clean
//! machine. Each now carries its own. Windows' words for an error ("The system cannot
//! find the file specified.") are GNU's (`No such file or directory`), and the messages
//! are shaped as GNU's are: each text below is Git Bash 5.3's.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, cash_command, run};

/// Every tool taken from uutils.
const TOOLS: [&str; 18] = [
    "cat", "cp", "head", "ln", "mkdir", "mv", "paste", "pr", "ptx", "rm", "rmdir", "shred", "shuf",
    "sort", "tac", "tail", "tee", "uniq",
];

#[test]
fn the_tools_taken_from_uutils_speak_english_not_message_ids() {
    for (script, want) in [
        ("sort /no/such/file", "sort: cannot read: /no/such/file:"),
        ("sort --help", "Usage: sort [OPTION]... [FILE]..."),
        ("tee --help", "Usage: tee [OPTION]... [FILE]..."),
        ("uniq --help", "Usage: uniq [OPTION]... [INPUT [OUTPUT]]"),
        (
            "shuf --help",
            "Shuffle the input by outputting a random permutation",
        ),
        ("shuf -r </dev/null", "shuf: no lines to repeat"),
        ("rm --help", "Usage: rm [OPTION]... FILE..."),
        (
            "rm /no/such",
            "rm: cannot remove '/no/such': No such file or directory",
        ),
        (
            "head /no/such",
            "head: cannot open '/no/such' for reading: No such file or directory",
        ),
        ("head -n x", "head: invalid number of lines: 'x'"),
        (
            "tail /no/such",
            "tail: cannot open '/no/such' for reading: No such file or directory",
        ),
        ("cp --help", "Usage: cp [OPTION]... [-T] SOURCE DEST"),
        ("mv --help", "Usage: mv [OPTION]... [-T] SOURCE DEST"),
        ("mkdir --help", "Usage: mkdir [OPTION]... DIRECTORY..."),
    ] {
        let out = run(script);
        let text = format!("{}{}", out.stdout, out.stderr);
        assert!(text.contains(want), "{script}: {text}");
        // An id is the tool's name, a dash and more words: `sort-cannot-read`.
        for tool in TOOLS {
            assert!(
                !text.contains(&format!("{tool}-usage"))
                    && !text.contains(&format!("{tool}-error")),
                "{script} printed a message id: {text}"
            );
        }
    }
}

/// Each tool's error on a file that is not there, or a like failure, in Git Bash's words.
#[test]
fn the_tools_taken_from_uutils_say_errors_in_gnus_words() {
    let scratch = Scratch::new("uutils-errors");
    std::fs::write(scratch.join("afile"), "x\n").unwrap();
    std::fs::create_dir(scratch.join("adir")).unwrap();
    for (script, want) in [
        ("cat nosuch", "cat: nosuch: No such file or directory"),
        (
            "cp nosuch x",
            "cp: cannot stat 'nosuch': No such file or directory",
        ),
        (
            "cp afile nodir/x",
            "cp: cannot create regular file 'nodir/x': No such file or directory",
        ),
        (
            "mv afile nodir/x",
            "mv: cannot move 'afile' to 'nodir/x': No such file or directory",
        ),
        (
            "ln nosuch z",
            "ln: failed to access 'nosuch': No such file or directory",
        ),
        (
            "mkdir adir",
            "mkdir: cannot create directory 'adir': File exists",
        ),
        (
            "mkdir nosuch/x",
            "mkdir: cannot create directory 'nosuch/x': No such file or directory",
        ),
        (
            "mkdir -p afile",
            "mkdir: cannot create directory 'afile': File exists",
        ),
        (
            "rmdir nosuch",
            "rmdir: failed to remove 'nosuch': No such file or directory",
        ),
        (
            "rmdir afile",
            "rmdir: failed to remove 'afile': Not a directory",
        ),
        (
            "tac nosuch",
            "tac: failed to open 'nosuch' for reading: No such file or directory",
        ),
        ("paste nosuch", "paste: nosuch: No such file or directory"),
        ("pr nosuch", "pr: nosuch: No such file or directory"),
        ("ptx nosuch", "ptx: nosuch: No such file or directory"),
        (
            "shred nosuch",
            "shred: nosuch: failed to open for writing: No such file or directory",
        ),
        (
            "sort nosuch",
            "sort: cannot read: nosuch: No such file or directory",
        ),
        (
            "tee nodir/x </dev/null",
            "tee: nodir/x: No such file or directory",
        ),
        (
            "stat nosuch",
            "stat: cannot stat 'nosuch': No such file or directory",
        ),
    ] {
        let out = cash_command()
            .current_dir(scratch.path())
            .args(["--noprofile", "--norc", "-c", script])
            .output()
            .unwrap();
        // 1, or 2 for sort, as in Git Bash.
        assert!(matches!(out.status.code(), Some(1 | 2)), "{script}");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(stderr.contains(want), "{script}: {stderr}");
        assert!(!stderr.contains("os error"), "{script}: {stderr}");
    }
}

#[test]
fn the_tools_taken_from_uutils_name_the_uutils_release_they_came_from() {
    // `sort --version` said `sort (uutils coreutils) 1.19.0`, cash's version, a uutils
    // release there never was; the bundled tools say 0.12.0, and so do these now.
    for tool in TOOLS {
        let out = run(&format!("{tool} --version"));
        assert_eq!(
            out.stdout.lines().next(),
            Some(format!("{tool} (uutils coreutils) 0.12.0").as_str()),
            "{}",
            out.stderr
        );
    }
}
