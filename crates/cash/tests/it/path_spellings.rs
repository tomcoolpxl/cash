//! Every way of writing a path, in every place a script writes one (spec D3): a table of
//! places by spellings, each cell a check. A folder named `my dir`, spelled `C:/…`,
//! `c:/…`, `C:\…`, `C:/…\…`, `/c/…`, `~/…`, relative, `./…`, `//server/share/…` and
//! `\\server\share\…` (the drive's admin share on `localhost`), goes to `cd`, `pushd`,
//! `cdh`, `CDPATH`, `z`, redirections, `source`, `test`, `ls`, globs, a script run by
//! its path, `PATH`, a bundled tool and a Windows program.
//!
//! A Unix spelling reaches a program as written, so `/c/…` given to `cat` or `git` is
//! the one documented exception: cash warns and names the `C:/` spelling. Tab and
//! Alt-E's start folder have tables of their own, in `completion_windows.rs` and
//! `cash-picker`'s `pick.rs`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::fmt::Write as _;
use std::process::Stdio;

use crate::common::{Scratch, cash_command};

/// A place a path is written, with `{S}` for the folder's spelling, and what its output
/// holds when the path worked.
const PLACES: [(&str, &str, &str); 17] = [
    ("cd", "cd {S} && pwd", "my dir"),
    ("pushd", "pushd {S} >/dev/null && pwd", "my dir"),
    ("cdh", "cdh {S} && pwd", "my dir"),
    (
        "CDPATH",
        "CDPATH={S}; cd sub >/dev/null && pwd",
        "my dir/sub",
    ),
    ("z", "z {S} && pwd", "my dir"),
    ("<", "read l < {S}/f.txt; echo \"$l\"", "hello"),
    (
        ">",
        "echo w > {S}/w.txt && read l < {S}/w.txt && echo \"$l\"",
        "w",
    ),
    (
        ">>",
        "echo a >> {S}/a.txt && echo b >> {S}/a.txt && wc -l < {S}/a.txt",
        "2",
    ),
    ("source", "source {S}/s.sh", "script-ran"),
    ("test", "[ -d {S} ] && test -f {S}/f.txt && echo yes", "yes"),
    ("ls", "ls {S}", "f.txt"),
    (
        "glob",
        "for f in {S}/*.txt; do [ -f \"$f\" ] && echo found; done",
        "found",
    ),
    ("glob-cat", "cat {S}/f.t?t", "hello"),
    ("run", "{S}/t.sh", "script-ran"),
    ("PATH", "PATH={S}:$PATH; t.sh", "script-ran"),
    ("cat", "cat {S}/f.txt", "hello"),
    ("git", "git hash-object {S}/f.txt", "ce01362"),
];

/// What `/c/…` given to a program gets: a warning naming the `C:/` spelling.
const WARNING: &str = "cash does not translate Unix path spellings in arguments";

#[test]
fn each_spelling_of_a_folder_works_in_each_place() {
    let scratch = Scratch::new("spellings");
    let home = scratch.join("home");
    let dir = home.join("my dir");
    std::fs::create_dir_all(dir.join("sub")).expect("create the folders");
    std::fs::write(dir.join("f.txt"), "hello\n").expect("write f.txt");
    std::fs::write(dir.join("s.sh"), "echo script-ran\n").expect("write s.sh");
    std::fs::write(dir.join("t.sh"), "#!/bin/sh\necho script-ran\n").expect("write t.sh");

    let fwd = dir.to_string_lossy().replace('\\', "/");
    let (drive, rest) = fwd.split_at(2);
    let letter = drive.get(..1).expect("a drive letter");
    let quoted = |text: &str| format!("'{text}'");
    let parent = fwd.trim_end_matches("/my dir");
    let spellings = [
        ("C:/x", quoted(&fwd)),
        ("c:/x", quoted(&format!("{}{rest}", drive.to_lowercase()))),
        ("C:\\x", quoted(&fwd.replace('/', "\\"))),
        ("mixed", quoted(&format!("{parent}\\my dir"))),
        ("/c/x", quoted(&format!("/{}{rest}", letter.to_lowercase()))),
        ("~/x", "~/'my dir'".to_owned()),
        ("rel", quoted("my dir")),
        ("./rel", "./'my dir'".to_owned()),
        ("//unc", quoted(&format!("//localhost/{letter}${rest}"))),
        (
            "\\\\unc",
            quoted(&format!(
                "\\\\localhost\\{letter}${}",
                rest.replace('/', "\\")
            )),
        ),
    ];

    let mut failed = Vec::new();
    for (name, spelled) in &spellings {
        let mut script = String::new();
        for (place, template, _) in PLACES {
            let command = template.replace("{S}", spelled);
            let _ = write!(script, "echo '@@{place}'\n( {command} ) 2>&1\n");
        }
        let out = cash_command()
            .args(["--norc", "--noprofile", "-c", &script])
            .current_dir(&home)
            .env("HOME", home.to_string_lossy().replace('\\', "/"))
            .env_remove("CDPATH")
            .stdin(Stdio::null())
            .output()
            .expect("run cash");
        let out = String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
        for (place, _, want) in PLACES {
            let section = out
                .split(&format!("@@{place}\n"))
                .nth(1)
                .and_then(|after| after.split("@@").next())
                .unwrap_or_default();
            // `/c/…` reaches a program as written, and cash says so.
            let documented = *name == "/c/x" && matches!(place, "cat" | "git");
            let worked = if documented {
                section.contains(WARNING)
            } else {
                section.contains(want)
            };
            if !worked {
                failed.push(format!("{place} with {name} ({spelled}):\n{section}"));
            }
        }
        // Each spelling ends where it started, so the next starts clean.
        for leftover in ["w.txt", "a.txt"] {
            let _ = std::fs::remove_file(dir.join(leftover));
        }
    }
    assert!(failed.is_empty(), "{}", failed.join("\n"));
}
