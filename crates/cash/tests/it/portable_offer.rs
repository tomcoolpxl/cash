//! The offer a portable cash makes once, at its first interactive prompt: to put cash on
//! the user PATH, add the Windows Terminal profile, and put its tools on PATH for other
//! programs. The test's `cash.exe` is copied into a scratch folder, which makes it
//! portable (neither Scoop's `scoop\apps\` nor the installer's version folder with a
//! `current` junction beside it), and started on a pseudo console. Each test has a home,
//! a `LOCALAPPDATA` and a user `Path` key of its own, so nothing of the developer's is
//! read or written.
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use cash_win32::conpty::ConPtySession;

use crate::common::{CASH, with_isolated_environment};

/// Registry key naming the installed fonts for a test: one that does not exist, so no
/// font is installed, whatever this machine has.
const NO_FONTS: &str = r"Software\cash-test-no-fonts";

/// How long the shell may take to reach its prompt, or a command to answer.
const STUCK: Duration = Duration::from_secs(20);

/// The first line of the offer.
const INTRO: &str = "This cash runs from the zip you unpacked";

/// Each question, by a part of it.
const ASK_PATH: &str = "Put cash on your PATH";
const ASK_PROFILE: &str = "profile to Windows Terminal";
const ASK_LINKS: &str = "on your PATH for other programs";

const PROMPT: &str = "PROMPT$";

/// `name` with this process's id and a count after it, so that no two folders, in this
/// run or another one at the same time, share a name.
fn unique(name: &str) -> String {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let count = COUNT.fetch_add(1, Ordering::Relaxed);
    format!("{name}-{}-{count}", std::process::id())
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// A path without the `\\?\` prefix, lowercased, for comparing.
fn plain(path: &Path) -> String {
    let text = path.to_string_lossy().replace('/', "\\");
    let text = text
        .strip_prefix(r"\\?\")
        .or_else(|| text.strip_prefix(r"\??\"))
        .unwrap_or(&text);
    text.trim_end_matches('\\').to_lowercase()
}

/// A portable cash: the test's `cash.exe` copied into a folder of its own, with a home,
/// a `LOCALAPPDATA` and a registry key standing in for the user's `Environment`; all
/// deleted when dropped. On the build's drive, as the tool links are hard links.
struct Portable {
    dir: PathBuf,
    exe: PathBuf,
    home: PathBuf,
    local: PathBuf,
    key_root: String,
    key: String,
}

impl Portable {
    /// The exe in `<dir>\unpacked\cash.exe`, as a zip unpacked by hand leaves it.
    fn new(name: &str) -> Self {
        Self::under(name, &["unpacked"])
    }

    /// The exe in `<dir>\<folders...>\cash.exe`.
    fn under(name: &str, folders: &[&str]) -> Self {
        let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("portable-offer")
            .join(unique(name));
        let _ = std::fs::remove_dir_all(&dir);
        let mut exe_dir = dir.clone();
        for folder in folders {
            exe_dir.push(folder);
        }
        let home = dir.join("home");
        let local = dir.join("local");
        for folder in [&exe_dir, &home, &local] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let exe = exe_dir.join("cash.exe");
        std::fs::copy(CASH, &exe).unwrap();
        let key_root = format!(r"Software\cash-test-portable-{}", unique(name));
        let key = format!(r"{key_root}\Environment");
        Self {
            dir,
            exe,
            home,
            local,
            key_root,
            key,
        }
    }

    /// The variables that make this cash's world its own, each with its value.
    fn variables(&self) -> Vec<(&'static str, String)> {
        let path = |path: &Path| path.to_string_lossy().into_owned();
        vec![
            ("HOME", path(&self.home)),
            ("USERPROFILE", path(&self.home)),
            ("LOCALAPPDATA", path(&self.local)),
            ("CASH_USER_ENVIRONMENT_KEY", self.key.clone()),
            ("CASH_FONTS_KEY", NO_FONTS.to_owned()),
            ("PS1", "PROMPT$ ".to_owned()),
            ("HISTFILE", String::new()),
        ]
    }

    /// An interactive cash on a pseudo console, reading no startup file, with `extra`
    /// in its environment on top of this install's.
    fn interactive(&self, extra: &[(&str, &str)]) -> ConPtySession {
        let own = self.variables();
        with_isolated_environment(|env| {
            let overridden = |name: &str| {
                name.eq_ignore_ascii_case("CASH_NO_OFFER")
                    || own.iter().any(|(each, _)| name.eq_ignore_ascii_case(each))
                    || extra
                        .iter()
                        .any(|(each, _)| name.eq_ignore_ascii_case(each))
            };
            let mut env: Vec<(&str, &str)> = env
                .iter()
                .copied()
                .filter(|(name, _)| !overridden(name))
                .collect();
            env.extend(own.iter().map(|(name, value)| (*name, value.as_str())));
            env.extend(extra.iter().copied());
            ConPtySession::start_in(
                &self.exe,
                &[
                    "--noprofile",
                    "--norc",
                    "--no-config",
                    "--disable-color",
                    "-i",
                ],
                Some(&env),
                Some(&self.dir),
            )
        })
        .expect("start cash in a pseudo terminal")
    }

    /// This cash, started with no console: its standard streams are pipes.
    fn command(&self) -> Command {
        let mut command = Command::new(&self.exe);
        command
            .current_dir(&self.dir)
            .env_remove("CASH_NO_OFFER")
            .envs(self.variables())
            .stdin(Stdio::null());
        command
    }

    fn marker(&self) -> PathBuf {
        self.local.join("cash").join("portable-offer")
    }

    fn fragment(&self) -> PathBuf {
        self.local
            .join("Microsoft")
            .join("Windows Terminal")
            .join("Fragments")
            .join("cash")
            .join("cash.json")
    }

    /// The links folder, `bin` beside the exe.
    fn bin(&self) -> PathBuf {
        self.exe.parent().unwrap().join("bin")
    }

    /// The stored user `Path`, as `reg query` prints it; `None` when there is none.
    fn user_path(&self) -> Option<String> {
        let out = Command::new("reg")
            .args(["query", &format!(r"HKCU\{}", self.key), "/v", "Path"])
            .stderr(Stdio::null())
            .output()
            .unwrap();
        let stdout = text(&out.stdout);
        let line = stdout
            .lines()
            .find(|line| line.trim_start().starts_with("Path"))?;
        let rest = line.trim_start().strip_prefix("Path")?.trim_start();
        let (_kind, value) = rest.split_once(char::is_whitespace)?;
        Some(value.trim().to_owned())
    }

    /// The entries of the stored user `Path`, lowercased.
    fn path_entries(&self) -> Vec<String> {
        self.user_path()
            .unwrap_or_default()
            .split(';')
            .filter(|each| !each.is_empty())
            .map(|each| each.trim_end_matches('\\').to_lowercase())
            .collect()
    }

    /// Nothing the offer could have set up is there.
    fn assert_untouched(&self) {
        assert_eq!(self.user_path(), None);
        assert!(!self.fragment().exists());
        assert!(!self.bin().exists());
    }
}

impl Drop for Portable {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = Command::new("reg")
            .args(["delete", &format!(r"HKCU\{}", self.key_root), "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Waits for the prompt, exits the shell and checks it ended well.
fn exit_at_prompt(mut session: ConPtySession) -> String {
    session.expect(PROMPT, STUCK).expect("the prompt");
    session.send("exit 0\r").unwrap();
    assert_eq!(session.wait().expect("cash exits"), 0);
    session.output().to_owned()
}

/// Reads the marker: the answers after the `asked=` line.
fn marker_lines(portable: &Portable) -> Vec<String> {
    let text = std::fs::read_to_string(portable.marker()).expect("the marker is written");
    let mut lines = text.lines().map(str::to_owned);
    let asked = lines.next().unwrap_or_default();
    assert!(
        asked.starts_with("asked=20") && asked.len() == "asked=2026-10-06".len(),
        "{text}"
    );
    lines.collect()
}

/// The report: the offer comes before the first prompt, Enter is no to each question,
/// nothing is set up, the answers are kept, and the next start makes no offer.
#[test]
fn the_offer_is_made_once_and_enter_means_no() {
    let portable = Portable::new("enter");
    let mut session = portable.interactive(&[]);
    session.expect(INTRO, STUCK).expect("the offer");
    for question in [ASK_PATH, ASK_PROFILE, ASK_LINKS] {
        session.expect(question, STUCK).expect("the question");
        session.send("\r").unwrap();
    }
    let output = exit_at_prompt(session);
    assert_eq!(output.matches("[y/N]").count(), 3, "{output}");
    assert!(
        output.find(INTRO).unwrap() < output.find(PROMPT).unwrap(),
        "the offer comes after the prompt:\n{output}"
    );

    portable.assert_untouched();
    assert_eq!(
        marker_lines(&portable),
        ["path=no", "profile=no", "links=no"]
    );

    // Asked and answered: the next shell goes straight to its prompt.
    let output = exit_at_prompt(portable.interactive(&[]));
    assert!(!output.contains(INTRO), "{output}");
    portable.assert_untouched();
}

/// The report: a yes to each question puts the exe's folder on the user PATH, writes
/// the Terminal profile naming this exe, and links the tools into `bin` beside it, first
/// on the user PATH; the commands say what they did, and the marker keeps the three yeses.
#[test]
fn yes_puts_cash_on_path_writes_the_profile_and_links_the_tools() {
    let portable = Portable::new("yes");
    let mut session = portable.interactive(&[]);
    for question in [ASK_PATH, ASK_PROFILE, ASK_LINKS] {
        session.expect(question, STUCK).expect("the question");
        session.send("y\r").unwrap();
    }
    let output = exit_at_prompt(session);
    assert!(
        output.contains("added to the front of your user PATH"),
        "{output}"
    );
    assert!(output.contains("cash --terminal-profile: "), "{output}");
    assert!(output.contains("cash --link-tools: "), "{output}");

    let exe_dir = plain(&std::fs::canonicalize(portable.exe.parent().unwrap()).unwrap());
    let bin = plain(&std::fs::canonicalize(portable.bin()).unwrap());
    let entries = portable.path_entries();
    assert_eq!(entries, [bin, exe_dir], "{entries:?}");

    let json = std::fs::read_to_string(portable.fragment()).unwrap();
    assert!(
        json.to_lowercase().contains(r"\\unpacked\\cash.exe"),
        "{json}"
    );
    assert!(portable.bin().join("ls.exe").is_file());
    assert!(portable.bin().join("sed.exe").is_file());
    assert!(portable.bin().join(".cash-links").is_file());

    assert_eq!(
        marker_lines(&portable),
        ["path=yes", "profile=yes", "links=yes"]
    );
}

/// The report: `CASH_NO_OFFER` in the environment skips the offer, and nothing records
/// that, so unsetting it brings the offer back.
#[test]
fn cash_no_offer_skips_the_offer_and_writes_no_marker() {
    let portable = Portable::new("skip");
    let output = exit_at_prompt(portable.interactive(&[("CASH_NO_OFFER", "1")]));
    assert!(!output.contains(INTRO), "{output}");
    assert!(!portable.marker().exists());
    portable.assert_untouched();

    let mut session = portable.interactive(&[]);
    session
        .expect(INTRO, STUCK)
        .expect("the offer, once the variable is gone");
    session.send("\x03").unwrap();
    exit_at_prompt(session);
}

/// The report: `-c`, a script and a shell reading a pipe make no offer and write no
/// marker, however the shell was asked to be interactive.
#[test]
fn a_command_a_script_and_a_pipe_get_no_offer() {
    let portable = Portable::new("no-console");
    let check = |out: Output, expected: &str| {
        let stdout = text(&out.stdout);
        let stderr = text(&out.stderr);
        assert!(out.status.success(), "{stderr}");
        assert_eq!(stdout.trim_end(), expected);
        assert!(!stderr.contains(INTRO), "{stderr}");
        assert!(!portable.marker().exists());
    };

    check(
        portable.command().args(["-c", "echo hi"]).output().unwrap(),
        "hi",
    );

    std::fs::write(portable.dir.join("script.sh"), "echo from-script\n").unwrap();
    check(
        portable.command().arg("script.sh").output().unwrap(),
        "from-script",
    );

    let mut child = portable
        .command()
        .args(["--norc", "--noprofile", "-i"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"echo piped\nexit 0\n")
        .unwrap();
    check(child.wait_with_output().unwrap(), "piped");
    portable.assert_untouched();
}

/// The report: a cash under `scoop\apps\` is Scoop's, whose install set things up, so
/// no offer.
#[test]
fn scoops_copy_gets_no_offer() {
    let portable = Portable::under("scoop", &["scoop", "apps", "cash", "current"]);
    let output = exit_at_prompt(portable.interactive(&[]));
    assert!(!output.contains(INTRO), "{output}");
    assert!(!portable.marker().exists());
    portable.assert_untouched();
}

/// The report: Ctrl-C at the first question is no to every question; the prompt
/// follows, nothing is set up, and the marker is written so the offer is not repeated.
#[test]
fn ctrl_c_at_the_first_question_is_no_to_all() {
    let portable = Portable::new("ctrl-c");
    let mut session = portable.interactive(&[]);
    session.expect(ASK_PATH, STUCK).expect("the first question");
    session.send("\x03").unwrap();
    let output = exit_at_prompt(session);
    assert!(!output.contains(ASK_PROFILE), "{output}");
    assert!(!output.contains(ASK_LINKS), "{output}");

    portable.assert_untouched();
    assert_eq!(
        marker_lines(&portable),
        ["path=no", "profile=no", "links=no"]
    );
}
