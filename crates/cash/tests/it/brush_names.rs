//! What a user sees names cash, not brush, whose code cash absorbed (ARCH-02, ARCH-09,
//! ARCH-10): the messages of the bundled tools' dispatch, `help` for a bundled tool, the
//! variables the shell sets, and the repository `--help` points at.

#![allow(
    clippy::tests_outside_test_module,
    reason = "an integration test is outside a test module by construction"
)]

use std::process::Command;

use crate::common::{CASH, output_of, run};

#[test]
fn help_for_a_bundled_tool_names_cash_and_ends_its_line() {
    // It said "executes via `brush --invoke-bundled cat`", and `help -d` ran into
    // whatever came next. The page is now the catalogue's, ending in the tool's own
    // `--help`.
    let out = run("help cat; help -d cat; echo next");
    assert!(!out.stdout.contains("brush"), "{}", out.stdout);
    assert!(!out.stdout.contains("invoke-bundled"), "{}", out.stdout);
    assert!(out.stdout.contains("Usage: cat"), "{}", out.stdout);
    assert!(
        out.stdout
            .ends_with("\ncat - Concatenate files to standard output.\nnext"),
        "{}",
        out.stdout
    );
}

#[test]
fn the_bundled_dispatch_speaks_as_cash() {
    // `--invoke-bundled` is read only as cash's first argument, so not `cash_command()`,
    // whose first is `--no-config`; the dispatch reads no config.
    let missing = output_of(Command::new(CASH).args(["--invoke-bundled", "no-such-tool"]));
    assert_eq!(
        missing.stderr,
        "cash: unknown bundled command: no-such-tool"
    );
    assert_eq!(missing.code, 127);

    let bare = output_of(Command::new(CASH).arg("--invoke-bundled"));
    assert_eq!(
        bare.stderr,
        "cash: --invoke-bundled requires a command name"
    );
    assert_eq!(bare.code, 2);
}

#[test]
fn the_shell_sets_cash_version_and_no_brush_variable() {
    let out = run(r#"echo "${CASH_VERSION:+set} ${BRUSH_VERSION-unset}""#);
    assert_eq!(out.stdout, "set unset");
}

#[test]
fn help_points_at_the_repository() {
    // `github.com/thraa/cash` does not exist; the repository is `tomcoolpxl/cash`.
    let out = output_of(Command::new(CASH).arg("--help"));
    assert!(
        out.stdout.contains("https://github.com/tomcoolpxl/cash"),
        "{}",
        out.stdout
    );
    assert!(!out.stdout.contains("thraa/cash"), "{}", out.stdout);
}
