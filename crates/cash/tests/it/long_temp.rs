//! A `TEMP` in 8.3 form. Windows gives `TEMP` and `TMP` as `C:\Users\RUNNER~1\…` for a
//! long user name, while `HOME` is `C:/Users/runneradmin`: a folder under `TEMP` was then
//! never shown under `~`, and one folder had two spellings in cash's records. cash takes
//! both to their long form as it starts, for itself and for what it starts.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    reason = "an integration test is outside a test module by construction"
)]

use crate::common::{Scratch, cash_command, output_of};

#[test]
fn a_short_temp_is_taken_to_its_long_form() {
    let scratch = Scratch::new("long-temp");
    // The test's own temp folder may be in 8.3 form itself, as on CI.
    let home = cash_win32::path::long_form(scratch.path().as_os_str())
        .map_or_else(|| scratch.path().to_path_buf(), std::path::PathBuf::from);
    let long = home.join("a long temporary folder");
    std::fs::create_dir(&long).expect("make the folder");
    let long_text = long.to_string_lossy().into_owned();
    let Some(short) = cash_win32::path::short_name(&long_text).filter(|s| *s != long_text) else {
        eprintln!("no 8.3 names on this volume: nothing to test");
        return;
    };

    let out = output_of(
        cash_command()
            .args([
                "-c",
                r#"echo "$TEMP"; echo "$TMP"; echo "$TMPDIR"; cmd /c 'echo %TEMP%'
cd /tmp && dirs"#,
            ])
            .env("HOME", &home)
            .env("TEMP", &short)
            .env("TMP", &short)
            .env_remove("TMPDIR"),
    );
    let lines: Vec<&str> = out.stdout.lines().map(str::trim_end).collect();
    assert_eq!(
        lines,
        [
            long_text.as_str(),
            &long_text,
            &long_text,
            &long_text,
            "~/a long temporary folder"
        ],
        "TEMP given as {short}; stderr: {}",
        out.stderr
    );
}
