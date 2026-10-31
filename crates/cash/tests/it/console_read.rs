//! `read -t` at a console: the line typed before its time ran out stays for the next
//! read, shown once, as a terminal keeps it (Git Bash 5.3 at a console, 2026-10-31:
//! `x=[] rc=142`, then `y=[abcdef]`, the screen showing `abcdef` once). cash puts the keys
//! back into the console's queue, and its next reader shows them no second time.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction"
)]

use std::time::Duration;

use crate::console_window::{
    let_go, screen_lines, start_cash_in_its_own_console, type_keys, wait_for_end,
};

#[test]
fn a_line_typed_before_read_t_ran_out_stays_for_the_next_read_shown_once() {
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("read.sh");
    let out = dir.path().join("out.txt");
    std::fs::write(
        &script,
        "exec > \"$1\" 2>&1\nread -t 3 x; echo \"x=[$x] rc=$?\"\nread y; echo \"y=[$y] rc=$?\"\n",
    )
    .unwrap();
    let process = start_cash_in_its_own_console(
        &format!(
            "--no-config --norc \"{}\" \"{}\"",
            script.display(),
            out.display()
        ),
        Duration::from_millis(800),
    );
    type_keys("abc");
    // The read's three seconds run out; then the rest of the line, and Enter.
    std::thread::sleep(Duration::from_millis(3500));
    type_keys("def\r");
    let ended = wait_for_end(&process, Duration::from_secs(10));
    let screen = screen_lines();
    let_go(&process);

    assert!(ended, "cash did not end; the screen shows {screen:?}");
    let output = std::fs::read_to_string(&out).unwrap().replace("\r\n", "\n");
    assert_eq!(
        output, "x=[] rc=142\ny=[abcdef] rc=0\n",
        "the screen shows {screen:?}"
    );
    // The keys were shown as typed, and not again when the second read took them back.
    let shown = screen
        .iter()
        .map(|line| line.matches("abc").count())
        .sum::<usize>();
    assert_eq!(shown, 1, "the screen shows {screen:?}");
    assert!(
        screen.iter().any(|line| line == "abcdef"),
        "the screen shows {screen:?}"
    );
}
