//! `where`, Windows' `where.exe` with dashes for options and cash's paths (D66). The
//! expectations were measured against `where.exe` on Windows 11.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::path::{Path, PathBuf};

use crate::common::{Scratch, cash_command};

/// A folder of files named like the ones `where.exe` was measured on, by its canonical
/// path: `%TEMP%` may be spelled with 8.3 names (`RUNNER~1` on CI), and a test runs `where`
/// in the folder and expects it printed in one spelling.
struct Fixture {
    real: PathBuf,
    _scratch: Scratch,
}

impl std::ops::Deref for Fixture {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.real
    }
}

fn fixture(name: &str) -> Fixture {
    let dir = Scratch::new(&format!("where-{name}"));
    for sub in ["sub/deep", "a_sub", "foo.exe.d"] {
        std::fs::create_dir_all(dir.join(sub)).unwrap();
    }
    for file in [
        "foo",
        "foo.bat",
        "foo.exe",
        "Foo.txt",
        "afoo.exe",
        "zfoo.exe",
        "LICENSE",
        "sub/foo.exe",
        "sub/deep/foo.cmd",
        "a_sub/foo.com",
    ] {
        std::fs::write(dir.join(file), "x").unwrap();
    }
    Fixture {
        real: std::fs::canonicalize(dir.path()).unwrap(),
        _scratch: dir,
    }
}

/// The folder as `where` prints it.
fn shown(dir: &Path) -> String {
    let text = dir.display().to_string().replace('\\', "/");
    text.strip_prefix("//?/").unwrap_or(&text).to_owned()
}

/// Run `script` in `dir`, with only System32 and `extra` on PATH.
fn run(dir: &Path, extra: &str, script: &str) -> (i32, String, String) {
    let path = format!(r"C:\Windows\System32;{extra}");
    let out = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .current_dir(dir)
        .env("PATH", path)
        .env("PATHEXT", ".COM;.EXE;.BAT;.CMD")
        .output()
        .unwrap();
    let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).replace("\r\n", "\n");
    (
        out.status.code().unwrap_or(-1),
        text(&out.stdout),
        text(&out.stderr),
    )
}

#[test]
fn a_bare_name_matches_itself_and_its_pathext_names_in_windows_order() {
    let dir = fixture("bare");
    let d = shown(&dir);
    let (status, stdout, _) = run(&dir, "", "where foo");
    assert_eq!(status, 0);
    assert_eq!(stdout, format!("{d}/foo\n{d}/foo.bat\n{d}/foo.exe\n"));

    // The current folder, then PATH; a PATH folder listed twice is searched once.
    let sub = format!("{d}/sub");
    let (_, stdout, _) = run(&dir, &format!("{sub};{sub}"), "where foo.exe");
    assert_eq!(stdout, format!("{d}/foo.exe\n{d}/sub/foo.exe\n"));
}

#[test]
fn wildcards_extensions_and_folders_match_as_in_where_exe() {
    let dir = fixture("wild");
    let d = shown(&dir);
    let (_, stdout, _) = run(&dir, "", "where '?foo.exe'");
    assert_eq!(stdout, format!("{d}/afoo.exe\n{d}/zfoo.exe\n"));

    // `NAME.*` matches `NAME` too, and a folder never matches.
    let (_, stdout, _) = run(&dir, "", "where 'foo.*'");
    assert_eq!(
        stdout,
        format!("{d}/foo\n{d}/foo.bat\n{d}/foo.exe\n{d}/Foo.txt\n")
    );
    let (status, _, stderr) = run(&dir, "", "where foo.exe.d");
    assert_eq!(status, 1);
    assert_eq!(
        stderr,
        "INFO: Could not find files for the given pattern(s).\n"
    );

    // A trailing dot means no extension, and a name without one is found as it is.
    let (_, stdout, _) = run(&dir, "", "where foo. LICENSE");
    assert_eq!(stdout, format!("{d}/foo\n{d}/LICENSE\n"));
}

#[test]
fn recursive_lists_a_folders_files_before_the_folders_below_it() {
    let dir = fixture("recursive");
    let d = shown(&dir);
    let (status, stdout, _) = run(&dir, "", "where -r . foo");
    assert_eq!(status, 0);
    assert_eq!(
        stdout,
        format!(
            "{d}/foo\n{d}/foo.bat\n{d}/foo.exe\n{d}/a_sub/foo.com\n{d}/sub/foo.exe\n{d}/sub/deep/foo.cmd\n"
        )
    );
    let (_, stdout, _) = run(&dir, "", "where --recursive sub/.. foo.exe");
    assert_eq!(stdout, format!("{d}/foo.exe\n{d}/sub/foo.exe\n"));
    let (_, stdout, _) = run(&dir, "", "where --recursive=sub foo.exe");
    assert_eq!(stdout, format!("{d}/sub/foo.exe\n"));
}

#[test]
fn folder_and_variable_forms_search_only_there() {
    let dir = fixture("forms");
    let d = shown(&dir);
    let (_, stdout, _) = run(&dir, "", "where sub:foo.exe");
    assert_eq!(stdout, format!("{d}/sub/foo.exe\n"));
    let (_, stdout, _) = run(&dir, "", "where 'sub;a_sub:foo.*'");
    assert_eq!(stdout, format!("{d}/sub/foo.exe\n{d}/a_sub/foo.com\n"));

    // A variable's name in any case, its value in either PATH form.
    let (_, stdout, _) = run(
        &dir,
        "",
        "export Places=\"$PWD/sub:$PWD/a_sub\"; where '$places:foo.*'",
    );
    assert_eq!(stdout, format!("{d}/sub/foo.exe\n{d}/a_sub/foo.com\n"));

    let (status, _, stderr) = run(&dir, "", "where '$NOPE_VAR:x'");
    assert_eq!(status, 1);
    assert_eq!(
        stderr,
        "ERROR: Environment variable \"NOPE_VAR\" is not found.\n\
         INFO: Could not find files for the given pattern(s).\n"
    );
}

#[test]
fn quiet_quote_and_times() {
    let dir = fixture("output");
    let d = shown(&dir);
    assert_eq!(
        run(&dir, "", "where -q foo"),
        (0, String::new(), String::new())
    );
    assert_eq!(
        run(&dir, "", "where -Q nope"),
        (1, String::new(), String::new())
    );

    let (_, stdout, _) = run(&dir, "", "where -f foo.exe");
    assert_eq!(stdout, format!("\"{d}/foo.exe\"\n"));

    // Size right-aligned in ten columns, the date in the user's short format, the time.
    let (_, stdout, _) = run(&dir, "", "where -tf foo.exe");
    let line = stdout.trim_end();
    assert!(line.starts_with("         1   "), "{line:?}");
    assert!(line.ends_with(&format!("  \"{d}/foo.exe\"")), "{line:?}");
    let middle: Vec<&str> = line.split_whitespace().collect();
    assert_eq!(middle.len(), 4, "{line:?}");
    assert_eq!(middle[2].split(':').count(), 3, "{line:?}");
}

#[test]
fn some_patterns_found_is_success_and_names_the_rest() {
    let dir = fixture("partial");
    let d = shown(&dir);
    let (status, stdout, stderr) = run(&dir, "", "where foo.exe nope foo.exe");
    assert_eq!(status, 0);
    // A repeated pattern is searched once.
    assert_eq!(stdout, format!("{d}/foo.exe\n"));
    assert_eq!(stderr, "INFO: Could not find \"nope\".\n");
}

#[test]
fn usage_errors_exit_2() {
    let dir = fixture("errors");
    let cases = [
        (
            "where -z x",
            "ERROR: Invalid argument or option - '-z'.\nType \"where --help\" for usage help.\n",
        ),
        (
            "where -r",
            "ERROR: Invalid syntax. Value expected for '-r'.\nType \"where --help\" for usage help.\n",
        ),
        (
            "where -r . x:y",
            "ERROR: \"path:pattern\" format cannot be used with -r.\n",
        ),
        (
            "where -r . '$PATH:x'",
            "ERROR: \"$env:pattern\" cannot be used with -r.\n",
        ),
        (
            "where sub:",
            "ERROR: Missing pattern in \"path:pattern\".\n",
        ),
        (
            "where 'C:\\Windows\\notepad.exe'",
            "ERROR: Invalid pattern is specified in \"path:pattern\".\n",
        ),
        (
            "where -r C:/nope_dir_for_where x",
            "ERROR: The system cannot find the file specified.\n",
        ),
        (
            "where ''",
            "ERROR: Value for default option cannot be empty.\nType \"where --help\" for usage help.\n",
        ),
    ];
    for (script, expected) in cases {
        let (status, stdout, stderr) = run(&dir, "", script);
        assert_eq!(
            (status, stdout.as_str(), stderr.as_str()),
            (2, "", expected),
            "{script}"
        );
    }
    let (status, _, stderr) = run(&dir, "", "where");
    assert_eq!(status, 2);
    assert!(stderr.starts_with("Usage: where"), "{stderr}");
}

#[test]
fn help_and_slash_options() {
    let dir = fixture("help");
    for script in ["where -?", "where --help", "where -q -? x"] {
        let (status, stdout, _) = run(&dir, "", script);
        assert_eq!(status, 0, "{script}");
        assert!(
            stdout.starts_with("Usage: where [-r DIR] [-q] [-f] [-t] PATTERN..."),
            "{script}"
        );
        assert!(stdout.contains("--recursive DIR"), "{script}");
    }

    // `/q` is a pattern, and the message says what to write instead.
    let (status, stdout, stderr) = run(&dir, "", "where /q foo.exe");
    assert_eq!(status, 0);
    assert_eq!(stdout, format!("{}/foo.exe\n", shown(&dir)));
    assert_eq!(
        stderr,
        "INFO: Could not find \"/q\".\n\
         INFO: \"/q\" was taken for a pattern; options take a dash here: -q\n"
    );
}
