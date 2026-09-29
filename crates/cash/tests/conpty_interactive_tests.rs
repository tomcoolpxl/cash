//! Native Win32 ConPTY interactive tests for cash.
//!
//! Spawns real `cash.exe` attached to a genuine Windows Pseudo Console (ConPTY)
//! with real terminal geometry (80x25), real VT100 / ANSI escape sequences,
//! and raw character I/O.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::single_char_pattern,
    clippy::literal_string_with_formatting_args,
    reason = "Integration tests test the compiled binary and assert loudly on failure."
)]

use cash_win32::conpty::ConPtySession;
use std::path::PathBuf;
use std::time::Duration;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn start_interactive_cash() -> ConPtySession {
    let cash_path = PathBuf::from(CASH);
    assert!(
        cash_path.exists(),
        "cash binary not found: {}",
        cash_path.display()
    );

    // Start cash with isolated flags and basic backend for automated ConPTY testing: no
    // startup file of whoever runs the tests, whose history format or banner would change
    // what the screen shows.
    ConPtySession::start(
        &cash_path,
        &[
            "--noprofile",
            "--norc",
            "--no-config",
            "--disable-bracketed-paste",
            "--disable-color",
            "--input-backend=basic",
            "-i",
        ],
        Some(&[("HISTFILE", "")]),
    )
    .expect("failed to start cash.exe attached to Win32 ConPTY")
}

/// Like [`start_interactive_cash`], on the reedline backend a user gets by default.
///
/// The session's environment is only what is listed here, so `TEMP` is passed on: without
/// it Windows puts temporary files in the Windows directory, where `fc` may not write.
fn start_reedline_cash() -> ConPtySession {
    let temp = std::env::temp_dir();
    let temp = temp.to_string_lossy();
    ConPtySession::start(
        &PathBuf::from(CASH),
        &[
            "--noprofile",
            "--norc",
            "--no-config",
            "--disable-color",
            "--input-backend=reedline",
            "-i",
        ],
        Some(&[("HISTFILE", ""), ("PS1", "PROMPT$ "), ("TEMP", &temp)]),
    )
    .expect("failed to start cash.exe attached to Win32 ConPTY")
}

/// A PowerShell script that leaves the console as a program that went wrong does, each
/// action exiting without putting anything back:
///
/// - `break` leaves it as `k3d cluster create` did (2026-09-29), and worse: VT input on,
///   VT processing off, line feeds no longer returning to the margin, code page 437;
/// - `alt` dies in the alternate screen of a full-screen program;
/// - `region` leaves a scroll region of rows 3 to 8;
/// - `mouse` takes the mouse as crossterm's mouse capture does, turning quick edit off;
/// - `early` turns VT input on and waits while keys are typed ahead;
/// - `input` prints the console's input mode, and anything else its state.
const CONSOLE_STATE_PS1: &str = r#"param([string]$What)
Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
public static class ConsoleState {
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
    static extern IntPtr CreateFileW(string n, uint a, uint s, IntPtr sa, uint d, uint f, IntPtr t);
    [DllImport("kernel32.dll")] static extern bool GetConsoleMode(IntPtr h, out uint m);
    [DllImport("kernel32.dll")] static extern bool SetConsoleMode(IntPtr h, uint m);
    [DllImport("kernel32.dll")] static extern bool SetConsoleCP(uint cp);
    [DllImport("kernel32.dll")] static extern bool SetConsoleOutputCP(uint cp);
    [DllImport("kernel32.dll")] static extern uint GetConsoleCP();
    [DllImport("kernel32.dll")] static extern uint GetConsoleOutputCP();
    static IntPtr Open(string n) { return CreateFileW(n, 0xC0000000u, 3, IntPtr.Zero, 3, 0, IntPtr.Zero); }
    public static void Break() {
        uint m;
        IntPtr i = Open("CONIN$"); GetConsoleMode(i, out m); SetConsoleMode(i, m | 0x200u);
        IntPtr o = Open("CONOUT$"); GetConsoleMode(o, out m); SetConsoleMode(o, (m & ~0x4u) | 0x8u);
        SetConsoleCP(437); SetConsoleOutputCP(437);
    }
    public static void VtInput() {
        uint m; IntPtr i = Open("CONIN$"); GetConsoleMode(i, out m); SetConsoleMode(i, m | 0x200u);
    }
    public static void TakeMouse() { SetConsoleMode(Open("CONIN$"), 0x98u); }
    public static string Input() {
        uint i; GetConsoleMode(Open("CONIN$"), out i); return "0x" + i.ToString("X4");
    }
    public static string Report() {
        uint i, o;
        GetConsoleMode(Open("CONIN$"), out i); GetConsoleMode(Open("CONOUT$"), out o);
        return "vt_input=" + ((i & 0x200u) != 0) + " vt_output=" + ((o & 0x4u) != 0)
            + " no_auto_return=" + ((o & 0x8u) != 0) + " cp=" + GetConsoleCP() + "/" + GetConsoleOutputCP();
    }
}
'@
$e = [char]27
switch ($What) {
    'break' { [ConsoleState]::Break(); [Console]::Out.WriteLine('BROKE_IT') }
    'alt' {
        [Console]::Out.Write("$e[?1049h$e[2J$e[5;10HFULL_SCREEN_TEXT$e[20;1H")
        [Console]::Out.WriteLine('LEFT_IN_FULL_SCREEN')
    }
    'region' { [Console]::Out.Write("$e[3;8r"); [Console]::Out.WriteLine('REGION_SET') }
    'mouse' { [ConsoleState]::TakeMouse(); [Console]::Out.WriteLine('TOOK_THE_MOUSE') }
    'early' {
        [ConsoleState]::VtInput()
        [Console]::Out.WriteLine('TYPE_NOW')
        Start-Sleep -Milliseconds 2500
    }
    'input' { [Console]::Out.WriteLine('INPUT_MODE=' + [ConsoleState]::Input()) }
    default { [Console]::Out.WriteLine('STATE ' + [ConsoleState]::Report()) }
}
"#;

/// A reedline cash on a ConPTY that runs [`CONSOLE_STATE_PS1`], with the environment
/// PowerShell needs to start at all.
struct ConsoleStateSession {
    session: ConPtySession,
    dir: PathBuf,
    pwsh: PathBuf,
}

impl ConsoleStateSession {
    fn start(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("cash-console-state-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("console-state.ps1"), CONSOLE_STATE_PS1).unwrap();

        // PowerShell 7 when installed, as on GitHub's runners; Windows PowerShell otherwise.
        let system_root = std::env::var("SystemRoot").unwrap_or_else(|_| r"C:\Windows".into());
        let pwsh = std::env::var("ProgramFiles")
            .map(|files| PathBuf::from(files).join(r"PowerShell\7\pwsh.exe"))
            .ok()
            .filter(|pwsh| pwsh.is_file())
            .unwrap_or_else(|| {
                PathBuf::from(&system_root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe")
            });

        let temp = std::env::temp_dir();
        let temp = temp.to_string_lossy();
        let path = std::env::var("PATH").unwrap_or_default();
        let local = std::env::var("LOCALAPPDATA").unwrap_or_default();
        let profile = std::env::var("USERPROFILE").unwrap_or_default();
        let mut session = ConPtySession::start(
            &PathBuf::from(CASH),
            &[
                "--noprofile",
                "--norc",
                "--no-config",
                "--disable-color",
                "--input-backend=reedline",
                "-i",
            ],
            Some(&[
                ("HISTFILE", ""),
                ("PS1", "PROMPT$ "),
                ("TEMP", &temp),
                ("TMP", &temp),
                ("SystemRoot", &system_root),
                ("PATH", &path),
                ("LOCALAPPDATA", &local),
                ("USERPROFILE", &profile),
            ]),
        )
        .expect("failed to start cash.exe attached to Win32 ConPTY");
        session
            .expect("PROMPT$", Duration::from_secs(10))
            .expect("prompt displayed");
        Self { session, dir, pwsh }
    }

    /// Types the command line that runs the script's action `what`.
    fn run(&mut self, what: &str) {
        let line = format!(
            "'{}' -NoProfile -ExecutionPolicy Bypass -File '{}' {what}\r",
            self.pwsh.display(),
            self.dir.join("console-state.ps1").display()
        );
        self.session.send(&line).unwrap();
    }

    /// Runs `what`, waits for the script's `marker` and then for the prompt after it.
    ///
    /// Keys typed while the script still runs are its own, and the console may turn them
    /// into VT text for it; so the next line is typed once the prompt is back, as a user
    /// would.
    fn run_to_prompt(&mut self, what: &str, marker: &str) {
        self.run(what);
        self.session
            .expect(marker, Duration::from_secs(30))
            .unwrap_or_else(|e| panic!("the script's {what} did not print {marker}: {e}"));
        self.wait_for_prompt_after(marker);
    }

    fn wait_for_prompt_after(&mut self, marker: &str) {
        let at = self.session.output().rfind(marker).unwrap();
        let start = std::time::Instant::now();
        while !self
            .session
            .output()
            .get(at..)
            .is_some_and(|after| after.contains("PROMPT$"))
        {
            assert!(
                start.elapsed() < Duration::from_secs(10),
                "no prompt after {marker}"
            );
            self.session.read_available().unwrap();
            std::thread::sleep(Duration::from_millis(15));
        }
    }

    /// The input mode the script's `input` action reports: a program sees the console's
    /// settings and the modes the line editor puts back for a command.
    fn input_mode(&mut self) -> String {
        self.run_to_prompt("input", "INPUT_MODE=0x");
        let output = self.session.output();
        let at = output.rfind("INPUT_MODE=0x").unwrap() + "INPUT_MODE=0x".len();
        output.get(at..at + 4).unwrap().to_owned()
    }

    /// What the screen shows, once output has stopped.
    fn screen(&mut self) -> String {
        self.session
            .settle(Duration::from_millis(500), Duration::from_secs(5))
            .unwrap();
        self.session.screen().text()
    }

    fn finish(mut self) {
        self.session.send("exit 0\r").unwrap();
        let code = self.session.wait().expect("process did not exit");
        assert_eq!(code, 0);
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// A program that leaves the console in a state the line editor cannot read is put right
/// before the next prompt (D68, decided with the user, 2026-09-29). `k3d cluster create`
/// left VT input on, and Enter and Backspace stopped working until the shell was closed.
#[test]
fn conpty_a_program_that_breaks_the_console_does_not_break_the_prompt() {
    let mut cash = ConsoleStateSession::start("break");
    cash.run_to_prompt("break", "BROKE_IT");

    // Enter must still end the line: without the repair it arrives as a bare `\r`, not
    // as the Enter key, and the line editor waits on.
    cash.session.send("echo SUM=$((40+2))\r").unwrap();
    cash.session
        .expect("SUM=42", Duration::from_secs(5))
        .expect("Enter did not reach the line editor after the console was broken");

    cash.run("report");
    cash.session
        .expect(
            "STATE vt_input=False vt_output=True no_auto_return=False cp=65001/65001",
            Duration::from_secs(30),
        )
        .expect("the console was not put back before the prompt");
    cash.finish();
}

/// A full-screen program that dies without leaving the alternate screen would have the
/// prompt drawn there, with everything before it out of sight (D68).
#[test]
fn conpty_a_program_that_dies_in_full_screen_leaves_the_prompt_on_the_main_screen() {
    let mut cash = ConsoleStateSession::start("alt");
    cash.session.send("echo MAIN_$((6*7))\r").unwrap();
    cash.session
        .expect("MAIN_42", Duration::from_secs(5))
        .unwrap();
    cash.run_to_prompt("alt", "LEFT_IN_FULL_SCREEN");
    cash.session.send("echo AFTER_$((6*7))\r").unwrap();
    cash.session
        .expect("AFTER_42", Duration::from_secs(5))
        .unwrap();

    let screen = cash.screen();
    assert!(
        screen.contains("MAIN_42") && !screen.contains("FULL_SCREEN_TEXT"),
        "the prompt stayed on the full-screen program's screen:\n{screen}"
    );
    cash.finish();
}

/// Leaving the alternate screen and resetting the scroll region move the cursor, and
/// after a healthy program neither may move the prompt (D68): a bare leave put the
/// output back into the line before it on a ConPTY.
#[test]
fn conpty_a_program_that_ran_leaves_a_healthy_prompt_where_it_was() {
    let mut cash = ConsoleStateSession::start("healthy");
    let mode = cash.input_mode();
    cash.session.send("echo HEALTHY_$((6*7))\r").unwrap();
    cash.session
        .expect("HEALTHY_42", Duration::from_secs(5))
        .unwrap();

    let screen = cash.screen();
    let lines: Vec<&str> = screen.lines().collect();
    let report = lines
        .iter()
        .position(|line| line.contains(&format!("INPUT_MODE=0x{mode}")))
        .unwrap_or_else(|| panic!("no report on the screen:\n{screen}"));
    assert_eq!(
        lines.get(report + 1..report + 3),
        Some(&["PROMPT$ echo HEALTHY_$((6*7))", "HEALTHY_42"][..]),
        "the prompt moved after the program:\n{screen}"
    );
    cash.finish();
}

/// A scroll region left behind confines everything after it to those rows, and output
/// above them is lost as it scrolls (D68).
#[test]
fn conpty_a_program_that_leaves_a_scroll_region_does_not_confine_later_output() {
    let mut cash = ConsoleStateSession::start("region");
    cash.run_to_prompt("region", "REGION_SET");
    let numbers: Vec<String> = (1..=30).map(|n| n.to_string()).collect();
    cash.session
        .send(&format!(
            "for i in {}; do echo L$i; done\r",
            numbers.join(" ")
        ))
        .unwrap();
    cash.session.expect("L30", Duration::from_secs(5)).unwrap();

    // Rows 3 to 8 hold six lines; the whole screen holds the last twenty or so.
    let screen = cash.screen();
    assert!(
        screen.lines().any(|line| line == "L10") && screen.lines().any(|line| line == "L30"),
        "output after the program still scrolled in its region:\n{screen}"
    );
    cash.finish();
}

/// A program that takes the mouse turns quick edit off, and Windows Terminal then sends
/// the tab the mouse instead of selecting text. The prompt puts back the settings cash
/// started with (D68).
#[test]
fn conpty_a_program_that_took_the_mouse_gives_it_back() {
    let mut cash = ConsoleStateSession::start("mouse");
    let before = cash.input_mode();
    let took = cash.session.output().len();
    cash.run_to_prompt("mouse", "TOOK_THE_MOUSE");
    let after = cash.input_mode();
    assert_eq!(
        after, before,
        "the console's input settings were not put back"
    );

    // Where the console asks the terminal for the mouse, it must also let it go.
    let stream = cash.session.output().get(took..).unwrap_or_default();
    if let Some(asked) = stream.rfind("\x1b[?1003;1006h") {
        assert!(
            stream
                .rfind("\x1b[?1003;1006l")
                .is_some_and(|gave| gave > asked),
            "the terminal was left sending the mouse"
        );
    }
    cash.finish();
}

/// Keys typed ahead while a program had VT input on reach the console as VT text: the
/// line editor decodes them (vendor/crossterm/CASH-PATCHES.md, patch 2), so the typed-ahead
/// command runs without a second Enter, and Backspace and the arrows edit it.
#[test]
fn conpty_a_program_that_had_vt_input_on_does_not_eat_keys_typed_ahead() {
    let mut cash = ConsoleStateSession::start("early");
    cash.run("early");
    cash.session
        .expect("TYPE_NOW", Duration::from_secs(30))
        .expect("the script did not turn VT input on");
    // X is rubbed out, and left then right leaves the cursor where it was.
    cash.session
        .send("echo EARLX\x7fY_\x1b[D\x1b[C$((1+1))\r")
        .unwrap();
    cash.session
        .expect("EARLY_2", Duration::from_secs(15))
        .expect("the command typed ahead did not run as typed");
    cash.finish();
}

// Keys that arrive after Enter belong to the command Enter starts. The line editor reads
/// the console in batches (vendor/crossterm/CASH-PATCHES.md), and a batch that ran past
/// Enter would keep the answer below from `read`, handing it to the next prompt instead.
#[test]
fn conpty_keys_after_enter_reach_the_command_it_runs() {
    let mut session = start_reedline_cash();
    // ConPTY does not send a trailing blank, so the prompt's space never arrives.
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    // One write, as a paste or a fast typist delivers it: the command, Enter, the answer.
    // The line after it waits in the console for the prompt `read` returns to.
    session.send("read -r answer\rTYPED_AHEAD\r").unwrap();
    session.send("echo \"GOT=[$answer]\"\r").unwrap();
    session
        .expect("GOT=[TYPED_AHEAD]", Duration::from_secs(10))
        .expect("read did not get the keys typed after its Enter");

    session.send("exit 0\r").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_startup_and_prompt() {
    let mut session = start_interactive_cash();

    // The shell starts and prompts for input.
    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("did not see prompt in ConPTY");

    // Send a simple arithmetic expansion command.
    session.send_line("echo RESULT=$((20 + 22))").unwrap();
    session.expect("RESULT=42", Duration::from_secs(5)).unwrap();

    // Exit cleanly with status 0.
    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_variables_and_arithmetic() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    session
        .send_line("FOO=conpty_value; echo \"VAL=$FOO\"")
        .unwrap();
    session
        .expect("VAL=conpty_value", Duration::from_secs(5))
        .unwrap();

    session.send_line("exit 17").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 17);
}

#[test]
fn conpty_interactive_history_and_pipeline() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Execute multiple commands.
    session.send_line("echo cmd_one").unwrap();
    session.expect("cmd_one", Duration::from_secs(5)).unwrap();

    session.send_line("echo cmd_two").unwrap();
    session.expect("cmd_two", Duration::from_secs(5)).unwrap();

    // Run history builtin and expect numbered history table entries.
    session.send_line("history").unwrap();
    session
        .expect("1  echo cmd_one", Duration::from_secs(5))
        .unwrap();
    session
        .expect("2  echo cmd_two", Duration::from_secs(5))
        .unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_multiline_block() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Multi-line loop.
    session
        .send_line("for x in alpha beta gamma; do echo \"ITEM:$x\"; done")
        .unwrap();
    session
        .expect("ITEM:alpha", Duration::from_secs(5))
        .unwrap();
    session.expect("ITEM:beta", Duration::from_secs(5)).unwrap();
    session
        .expect("ITEM:gamma", Duration::from_secs(5))
        .unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_history_expansion_bang_dollar() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Run initial command with arguments.
    session.send_line("echo alpha beta_target").unwrap();
    session
        .expect("alpha beta_target", Duration::from_secs(5))
        .unwrap();

    // Verify !$ expands to last argument of previous command.
    session.send_line("echo EXP_DOLLAR=!$").unwrap();
    session
        .expect("echo EXP_DOLLAR=beta_target", Duration::from_secs(5))
        .unwrap();
    session
        .expect("EXP_DOLLAR=beta_target", Duration::from_secs(5))
        .unwrap();

    // Verify !^ expands to first argument of previous command (EXP_DOLLAR=beta_target).
    session.send_line("echo EXP_CARET=!^").unwrap();
    session
        .expect(
            "echo EXP_CARET=EXP_DOLLAR=beta_target",
            Duration::from_secs(5),
        )
        .unwrap();

    // Verify quick substitution ^old^new^.
    session.send_line("echo hello world").unwrap();
    session
        .expect("hello world", Duration::from_secs(5))
        .unwrap();
    session.send_line("^world^cash_user^").unwrap();
    session
        .expect("echo hello cash_user", Duration::from_secs(5))
        .unwrap();
    session
        .expect("hello cash_user", Duration::from_secs(5))
        .unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_interactive_empty_history_bang_dollar() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // When history is empty, typing `ls !$` must output event not found and NOT pass '!$' as a literal argument to ls.
    session.send_line("ls !$").unwrap();
    session
        .expect("cash: !$: event not found", Duration::from_secs(5))
        .unwrap();

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_read_e_edits_initial_text() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    // Send one Enter key. `send_line` writes CRLF, which ConPTY can expose as two
    // key events when the command switches the console into raw mode immediately.
    session
        .send(concat!(
            r#"read -e -p "EDIT> " -i ac value; echo "READ=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("EDIT> ac", Duration::from_secs(5))
        .expect("read did not display its prompt and initial text");

    // Move between 'a' and 'c', insert 'b', and submit the edited buffer.
    session.send("\x1b[Db\r").unwrap();
    session
        .expect("READ=[abc]", Duration::from_secs(5))
        .expect("read -e did not return the edited text");

    session
        .send(concat!(
            r#"read -e -p "EDIT2> " -i axbc value; echo "EDIT2=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("EDIT2> axbc", Duration::from_secs(5))
        .expect("second read did not start");

    // Home, Right, Delete, End, Backspace, then replace the final character.
    session.send("\x1b[H\x1b[C\x1b[3~\x1b[F\x7fc\r").unwrap();
    session
        .expect("EDIT2=[abc]", Duration::from_secs(5))
        .expect("read -e navigation and deletion produced the wrong text");

    session
        .send(concat!(
            r#"read -e -d : -p "DELIM> " value; echo "DELIM=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("DELIM> ", Duration::from_secs(5))
        .expect("delimiter read did not start");
    session.send("a\\:b:").unwrap();
    session
        .expect("DELIM=[a:b]", Duration::from_secs(5))
        .expect("read -e did not preserve an escaped custom delimiter");

    // Bash only uses -i when Readline was requested with -e or -E.
    session
        .send(concat!(
            r#"read -i seed -p "PLAIN> " value; echo "PLAIN=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("PLAIN> ", Duration::from_secs(5))
        .expect("plain read did not start");
    session.send("actual\r").unwrap();
    session
        .expect("PLAIN=[actual]", Duration::from_secs(5))
        .expect("read -i incorrectly applied initial text without -e or -E");

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_read_e_navigates_in_memory_history() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    session.send_line("history -s first-entry").unwrap();
    session.send_line("history -s second-entry").unwrap();
    session
        .send(concat!(
            r#"read -e -p "HIST> " value; echo "HISTORY=[$value]""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("HIST> ", Duration::from_secs(5))
        .expect("read history prompt did not appear");

    // The command containing `read` is itself the newest history entry. Walk past it
    // to the two entries injected above, then forward once with Ctrl-N.
    session.send("\x10\x10\x10\x0e\r").unwrap();
    session
        .expect("HISTORY=[second-entry]", Duration::from_secs(5))
        .expect("read -e did not navigate the shell's in-memory history");

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

#[test]
fn conpty_read_capital_e_uses_shell_completion() {
    let mut session = start_interactive_cash();

    session
        .expect("cash", Duration::from_secs(5))
        .or_else(|_| session.expect("$", Duration::from_secs(2)))
        .expect("prompt displayed");

    session
        .send(concat!(
            r#"read -E -p "COMPLETE> " -i ech value; printf 'COMPLETED=[%s]\n' "$value""#,
            "\r"
        ))
        .unwrap();
    session
        .expect("COMPLETE> ech", Duration::from_secs(5))
        .expect("read completion prompt did not appear");
    session.send("\t\r").unwrap();
    session
        .expect("COMPLETED=[echo]", Duration::from_secs(5))
        .expect("read -E did not use the shell completion engine");

    session.send_line("exit 0").unwrap();
    let code = session.wait().expect("process did not exit");
    assert_eq!(code, 0);
}

/// Readline's `yank-last-arg`: Alt-. inserts the previous command's last word, and a second
/// press replaces it with the last word of the command before that.
#[test]
fn conpty_alt_dot_yanks_the_last_argument() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    session.send("true first OLDER_WORD\r").unwrap();
    session.send("true second NEWER_WORD\r").unwrap();

    // The output joins the words with `-`, which the typed line never contains.
    session.send("printf '%s-%s\\n' X \x1b.\r").unwrap();
    session
        .expect("X-NEWER_WORD", Duration::from_secs(10))
        .expect("Alt-. did not insert the last word of the previous command");

    // Now the printf is the previous command: the third press reaches the first `true`.
    session
        .send("printf '%s+%s\\n' Y \x1b.\x1b.\x1b.\r")
        .unwrap();
    session
        .expect("Y+OLDER_WORD", Duration::from_secs(10))
        .expect("repeated Alt-. did not walk back through history");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// Readline's `edit-and-execute-command`: Ctrl-X Ctrl-E opens the line in `$VISUAL` and
/// runs what the editor saves. The "editor" here is cash's `sed -i`.
#[test]
fn conpty_ctrl_x_ctrl_e_runs_the_edited_line() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    session
        .send("export VISUAL='sed -i s/ORIGINAL/EDITED/'\r")
        .unwrap();
    session.send("echo ORIGINAL$((20 + 3))\x18\x05").unwrap();
    session
        .expect("EDITED23", Duration::from_secs(10))
        .expect("Ctrl-X Ctrl-E did not run the edited line");

    // History holds the line as typed and as edited, as Bash's does, and not the `fc`.
    session
        .send("history 3 | sed 's/^ *[0-9]* *//; s/ /_/g'\r")
        .unwrap();
    session
        .expect("echo_EDITED$((20_+_3))", Duration::from_secs(10))
        .expect("the edited line was not recorded in history");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// fish's abbreviations (D60): the command word expands on Space and on Enter, history
/// keeps the expansion, and an erased abbreviation expands no more.
#[test]
fn conpty_abbr_expands_as_the_command_word() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    // Quoted whole, so the expansion keeps its own quotes.
    session.send("abbr -a pj \"printf '%s-%s\\n'\"\r").unwrap();

    // Each check looks for output that no typed line contains, so it holds however the
    // terminal happens to draw the lines around it.

    // On Space: the typed line becomes `printf '%s-%s\n' A B`. The keys go in one write,
    // so they are read together.
    session.send("pj A B\r").unwrap();
    session
        .expect("A-B", Duration::from_secs(10))
        .expect("the abbreviation did not expand on Space");

    // Not as an argument: printed as it was typed, in brackets.
    session.send("printf '<%s>\\n' pj\r").unwrap();
    session
        .expect("<pj>", Duration::from_secs(10))
        .expect("an argument expanded");

    // On Enter, with nothing after it.
    session
        .send("abbr -a pk \"printf 'K%s%s\\n' x y\"\r")
        .unwrap();
    session.send("pk\r").unwrap();
    session
        .expect("Kxy", Duration::from_secs(10))
        .expect("the abbreviation did not expand on Enter");

    // The session has no PATH, so cash's own awk: history lines whose command is the
    // expanded `printf 'K…`.
    session
        .send("history 5 | awk '$2 == \"printf\" && /K%s/ {n++} END {print \"HIST=\" n}'\r")
        .unwrap();
    session
        .expect("HIST=1", Duration::from_secs(10))
        .expect("history did not keep the expansion");

    session.send("abbr -e pj\r").unwrap();
    session.send("pj D E\r").unwrap();
    session
        .expect("command not found: pj", Duration::from_secs(10))
        .expect("an erased abbreviation still expanded");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// The collapsing prompt (D61): with `CASH_TRANSIENT_PS1` set, an entered line keeps that
/// prompt, expanded like PS1, instead of the full one; unset, prompts stay as they were.
#[test]
fn conpty_transient_prompt_replaces_an_entered_lines_prompt() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    session.send("CASH_TRANSIENT_PS1='\\s> '\r").unwrap();
    session.send("echo COLLAPSED_$((6 * 7))\r").unwrap();
    session
        .expect("COLLAPSED_42", Duration::from_secs(10))
        .expect("command did not run");
    session.send("unset CASH_TRANSIENT_PS1\r").unwrap();
    session.send("echo FULL_$((6 * 8))\r").unwrap();
    session
        .expect("FULL_48", Duration::from_secs(10))
        .expect("command did not run");
    session
        .settle(Duration::from_millis(300), Duration::from_secs(5))
        .unwrap();

    let screen = session.screen().text();
    let line_of = |needle: &str| {
        screen
            .lines()
            .find(|line| line.contains(needle))
            .unwrap_or_default()
            .trim_end()
            .to_owned()
    };
    // The line that set it was entered at the full prompt; the next one collapses. The
    // escape is `\s`, the shell's name: `\$` is `#` for an administrator, as CI runs.
    assert_eq!(
        line_of("CASH_TRANSIENT_PS1="),
        "PROMPT$ CASH_TRANSIENT_PS1='\\s> '",
        "{screen}"
    );
    assert_eq!(
        line_of("echo COLLAPSED"),
        "cash> echo COLLAPSED_$((6 * 7))",
        "{screen}"
    );
    assert_eq!(
        line_of("echo FULL"),
        "PROMPT$ echo FULL_$((6 * 8))",
        "{screen}"
    );

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// Keys typed right after a `bind -x` key, read in the same batch, reach the next prompt;
/// the line editor dropped them (vendor/reedline/CASH-PATCHES.md, patch 5).
#[test]
fn conpty_keys_after_a_bound_key_reach_the_next_prompt() {
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");
    session.send("bind -x '\"\\C-t\": true'\r").unwrap();
    session
        .settle(Duration::from_millis(300), Duration::from_secs(5))
        .unwrap();

    // One write: the bound key, then a whole command typed ahead.
    session.send("\x14echo TYPED_AHEAD_$((2 + 2))\r").unwrap();
    session
        .expect("TYPED_AHEAD_4", Duration::from_secs(10))
        .expect("keys typed after a bound key were lost");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}

/// fish's folder history on the keys (D62): Alt-← and Alt-→ on an empty line go back and
/// forward through the folders visited; on a line with text they move a word.
#[test]
fn conpty_alt_arrows_walk_the_folder_history() {
    const ALT_LEFT: &str = "\x1b[1;3D";
    const ALT_RIGHT: &str = "\x1b[1;3C";

    let root = tempfile::tempdir().unwrap();
    for name in ["first_dir", "second_dir"] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }
    let mut session = start_reedline_cash();
    session
        .expect("PROMPT$", Duration::from_secs(10))
        .expect("prompt displayed");

    let root = root.path().to_string_lossy().replace('\\', "/");
    session
        .send(&format!("cd '{root}/first_dir'; cd ../second_dir\r"))
        .unwrap();
    session
        .send("here() { echo \"AT=${PWD##*/}\"; }\r")
        .unwrap();

    session.send(ALT_LEFT).unwrap();
    session.send("here\r").unwrap();
    session
        .expect("AT=first_dir", Duration::from_secs(10))
        .expect("Alt-Left on an empty line did not go back");

    session.send(ALT_RIGHT).unwrap();
    session.send("here\r").unwrap();
    session
        .expect("AT=second_dir", Duration::from_secs(10))
        .expect("Alt-Right on an empty line did not go forward");

    // With text on the line, Alt-Left moves to the start of the last word.
    session.send("echo abc def").unwrap();
    session.send(ALT_LEFT).unwrap();
    session.send("Z\r").unwrap();
    session
        .expect("abc Zdef", Duration::from_secs(10))
        .expect("Alt-Left on a line with text did not move a word");
    // A marker of its own: `AT=second_dir` is on the screen already.
    session.send("echo \"STILL=${PWD##*/}\"\r").unwrap();
    session
        .expect("STILL=second_dir", Duration::from_secs(10))
        .expect("Alt-Left on a line with text changed folder");

    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("process did not exit"), 0);
}
