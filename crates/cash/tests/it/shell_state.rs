//! The shell's state is the shell's, not the process's (D10).
//!
//! cash never changes its own working directory, and `export` never touches the process
//! environment: both live in the shell, so that a subshell or a background job can have
//! its own. Anything that reaches for the process's copy instead gets the folder cash was
//! started in and the environment it was started with (`REVIEW_REPORT.md` §4.1).

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use std::path::Path;
use std::process::Command;

use crate::common::{Scratch, run_in};

/// What `program` from System32 prints, run directly.
fn output_of_system_program(program: &str) -> String {
    let path = Path::new(r"C:\Windows\System32").join(program);
    let out = Command::new(path).output().unwrap();
    String::from_utf8_lossy(&out.stdout).trim_end().to_owned()
}

#[test]
fn a_relative_program_runs_from_the_shells_folder() {
    // Two programs of one name, `tool.exe` in the folder cash starts in and in `sub`.
    // They print different things, so the output says which one ran.
    let scratch = Scratch::new("relative-program");
    std::fs::create_dir(scratch.path().join("sub")).unwrap();
    std::fs::copy(
        r"C:\Windows\System32\whoami.exe",
        scratch.path().join("tool.exe"),
    )
    .unwrap();
    std::fs::copy(
        r"C:\Windows\System32\hostname.exe",
        scratch.path().join("sub").join("tool.exe"),
    )
    .unwrap();
    std::fs::copy(
        r"C:\Windows\System32\hostname.exe",
        scratch.path().join("sub").join("only-here.exe"),
    )
    .unwrap();
    let hostname = output_of_system_program("hostname.exe");

    // `cd sub && ./tool.exe` ran the start folder's `tool.exe`, and `./only-here.exe` was
    // "command not found", because Windows resolved them against cash's own folder.
    let out = run_in(
        scratch.path(),
        "cd sub && ./tool.exe && ./tool && ./only-here.exe",
    );
    let lines: Vec<&str> = out.stdout.lines().map(str::trim_end).collect();
    assert_eq!(lines, [hostname.as_str(); 3], "{}", out.stderr);

    // A program named with forward slashes is started by its Windows spelling: cmd.exe
    // read the `/` in its own command line as a switch.
    let out = run_in(
        scratch.path(),
        "C:/Windows/System32/cmd.exe /d /c exit 4; echo $?",
    );
    assert_eq!(out.stdout, "4", "{}", out.stderr);

    // A relative entry in PATH is the shell's folder's too.
    let out = run_in(scratch.path(), r#"PATH=".:$PATH"; cd sub && only-here"#);
    assert_eq!(out.stdout, hostname, "{}", out.stderr);
}

/// A scratch folder with `sub/a.txt` and `show.cmd`, a batch file that prints `$XF` and
/// its first argument, found by its bare name through a `bin` folder put on `PATH`.
fn batch_scratch(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    std::fs::create_dir_all(scratch.path().join("sub")).unwrap();
    std::fs::create_dir_all(scratch.path().join("bin")).unwrap();
    std::fs::write(scratch.path().join("sub").join("a.txt"), "a").unwrap();
    std::fs::write(
        scratch.path().join("bin").join("show.cmd"),
        "@echo [%XF%] [%1]\r\n",
    )
    .unwrap();
    scratch
}

/// `bin` on `PATH`, `XF` exported, and the shell in `sub`.
const SETUP: &str = r#"PATH="$PWD/bin:$PATH"; export XF=bar; cd sub"#;

#[test]
fn xargs_runs_a_command_as_the_shell_does() {
    // The exported variable, the shell's PATH, a `.cmd` by its bare name: each was lost
    // when xargs started the command itself.
    let scratch = batch_scratch("xargs-shell");
    let out = run_in(scratch.path(), &format!("{SETUP}; echo hi | xargs show"));
    assert_eq!(out.stdout.trim_end(), "[bar] [hi]", "{}", out.stderr);

    let out = run_in(scratch.path(), "echo x | xargs nosuchcmd; echo \"rc=$?\"");
    assert_eq!(out.stdout, "rc=127", "{}", out.stderr);
    assert!(
        out.stderr.contains("xargs: nosuchcmd: command not found"),
        "{}",
        out.stderr
    );
}

#[test]
fn xargs_gives_the_command_an_empty_input() {
    // What xargs reads is its own, as in GNU xargs: the command used to read cash's.
    let scratch = Scratch::new("xargs-stdin");
    let input = scratch.path().join("input.txt");
    std::fs::write(&input, "SECRET\n").unwrap();
    let out = crate::common::output_of(
        crate::common::cash_command()
            .args(["-c", "printf \"\" | xargs C:/Windows/System32/sort.exe"])
            .stdin(std::fs::File::open(&input).unwrap()),
    );
    assert_eq!(out.stdout, "", "{}", out.stderr);
}

#[test]
fn find_exec_runs_a_command_as_the_shell_does() {
    let scratch = batch_scratch("find-exec-shell");
    let out = run_in(
        scratch.path(),
        &format!(r"{SETUP}; find . -name a.txt -exec show {{}} \; > out.txt; cat out.txt"),
    );
    // The output went where find's goes, and the command saw the shell's state.
    assert_eq!(out.stdout.trim_end(), "[bar] [./a.txt]", "{}", out.stderr);

    let out = run_in(
        scratch.path(),
        r"find . -name a.txt -exec nosuchcmd {} \;; echo rc=$?",
    );
    assert_eq!(out.stdout, "rc=1", "{}", out.stderr);
    assert!(
        out.stderr.contains("find: nosuchcmd: command not found"),
        "{}",
        out.stderr
    );
}

#[test]
fn nohup_runs_a_command_as_the_shell_does() {
    let scratch = batch_scratch("nohup-shell");
    let out = run_in(
        scratch.path(),
        &format!(
            r#"{SETUP}; nohup sh -c 'echo "[$XF]"; pwd' > out.txt; echo "rc=$?"; cat out.txt"#
        ),
    );
    let sub = scratch
        .path()
        .join("sub")
        .to_string_lossy()
        .replace('\\', "/");
    assert_eq!(out.stdout, format!("rc=0\n[bar]\n{sub}"), "{}", out.stderr);
}
