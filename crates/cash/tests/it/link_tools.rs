//! `cash --link-tools [--add-to-path] [DIR]` and `cash --unlink-tools [DIR]` (ROADMAP
//! items 16 and 18, D65): hard links to `cash.exe`, one per tool, that programs outside
//! cash can run, and the user PATH entry that lets them.
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    reason = "an integration test is outside a test module by construction, and a failed \
              assumption in a test should abort it loudly"
)]

use std::cell::RefCell;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::{Mutex, MutexGuard, PoisonError};

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// NTFS gives a file at most 1023 names, and each links folder gives the test's
/// `cash.exe` 126 more. So the tests take turns: a test holds the turn while a folder of
/// its own exists, and a folder is deleted when its test is done with it.
static TURN: Mutex<()> = Mutex::new(());

thread_local! {
    /// This test's folders, and its turn while it has any.
    static HELD: RefCell<(usize, Option<MutexGuard<'static, ()>>)> =
        const { RefCell::new((0, None)) };
}

/// A fresh folder on the same drive as the test's `cash.exe`, which a hard link needs;
/// deleted when dropped.
struct Folder(PathBuf);

fn folder(name: &str) -> Folder {
    HELD.with_borrow_mut(|(count, turn)| {
        if *count == 0 {
            *turn = Some(TURN.lock().unwrap_or_else(PoisonError::into_inner));
        }
        *count += 1;
    });
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("link_tools")
        .join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    Folder(dir)
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
        HELD.with_borrow_mut(|(count, turn)| {
            *count -= 1;
            if *count == 0 {
                *turn = None;
            }
        });
    }
}

impl std::ops::Deref for Folder {
    type Target = Path;
    fn deref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<OsStr> for Folder {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

fn link_tools(dir: &Path) -> Output {
    Command::new(CASH)
        .arg("--link-tools")
        .arg(dir)
        .stdin(Stdio::null())
        .output()
        .unwrap()
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

/// This process's PATH with `dir` in front.
fn path_with(dir: &Path) -> String {
    format!(
        "{};{}",
        dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

/// Run a program with `input` on its standard input, from outside any shell.
fn run(program: &Path, args: &[&str], input: &str) -> Output {
    use std::io::Write as _;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn links_every_tool_and_writes_the_manifest() {
    let dir = folder("every");
    let out = link_tools(&dir);
    assert!(out.status.success(), "{}", text(&out.stderr));
    let stdout = text(&out.stdout);
    assert!(stdout.contains(" linked, 0 refreshed"), "{stdout}");

    let manifest = std::fs::read_to_string(dir.join(".cash-links")).unwrap();
    let names: Vec<&str> = manifest.lines().collect();
    for tool in ["ls", "sort", "wc", "xargs", "awk", "sed", "uname"] {
        assert!(names.contains(&tool), "{tool} missing from {manifest}");
        assert!(dir.join(format!("{tool}.exe")).is_file(), "{tool}.exe");
    }
    // Bash's own builtins have no file to run, and `[` no file name.
    for builtin in ["cd", "echo", "test", "["] {
        assert!(!names.contains(&builtin), "{builtin} in {manifest}");
    }
}

#[test]
fn a_link_runs_as_its_tool_outside_cash() {
    let dir = folder("runs");
    assert!(link_tools(&dir).status.success());

    // A bundled tool, which cash runs by re-entering its own exe: here, the link.
    let sort = run(&dir.join("sort.exe"), &[], "b\na\n");
    assert_eq!(text(&sort.stdout), "a\nb\n", "{}", text(&sort.stderr));
    assert!(sort.status.success());

    let wc = run(&dir.join("wc.exe"), &["-l"], "1\n2\n3\n");
    assert_eq!(text(&wc.stdout).trim(), "3");

    let uname = run(&dir.join("uname.exe"), &["-s"], "");
    assert_eq!(text(&uname.stdout), "Windows_NT\n");

    let basename = run(&dir.join("basename.exe"), &["a/b/c.txt", ".txt"], "");
    assert_eq!(text(&basename.stdout), "c\n");

    // A tool's own status is the process's.
    let ls = run(&dir.join("ls.exe"), &["no-such-file-here"], "");
    assert_eq!(ls.status.code(), Some(2), "{}", text(&ls.stderr));

    // `whoami` is also a link here: cash, coming up as any tool, must not start the
    // `whoami.exe` beside it for the user's SID, or each start starts another.
    let whoami = run(&dir.join("whoami.exe"), &[], "");
    assert!(whoami.status.success(), "{}", text(&whoami.stderr));
    assert!(!text(&whoami.stdout).trim().is_empty());
}

#[test]
fn a_link_that_runs_commands_runs_them() {
    let dir = folder("commands");
    assert!(link_tools(&dir).status.success());

    let xargs = run(&dir.join("xargs.exe"), &["-n1", "basename"], "a/b c/d\n");
    assert_eq!(text(&xargs.stdout), "b\nd\n", "{}", text(&xargs.stderr));

    let timeout = run(&dir.join("timeout.exe"), &["10", "sort"], "y\nx\n");
    assert_eq!(text(&timeout.stdout), "x\ny\n", "{}", text(&timeout.stderr));
}

#[test]
fn rerunning_keeps_current_links_and_leaves_other_files_alone() {
    let dir = folder("rerun");
    std::fs::write(dir.join("ls.exe"), "not cash").unwrap();
    let first = text(&link_tools(&dir).stdout);
    assert!(
        first.contains("left alone, not cash's links: ls.exe"),
        "{first}"
    );
    assert_eq!(std::fs::read(dir.join("ls.exe")).unwrap(), b"not cash");

    let again = link_tools(&dir);
    assert!(again.status.success());
    let again = text(&again.stdout);
    assert!(again.contains("all already linked"), "{again}");
    assert!(
        again.contains("left alone, not cash's links: ls.exe"),
        "{again}"
    );
}

#[test]
fn a_replaced_link_is_refreshed_and_doctor_reports_it_first() {
    let dir = folder("stale");
    assert!(link_tools(&dir).status.success());

    // What an upgrade leaves: a link to a cash.exe that is no longer this one.
    std::fs::remove_file(dir.join("wc.exe")).unwrap();
    std::fs::copy(CASH, dir.join("wc.exe")).unwrap();

    let path = path_with(&dir);
    let doctor = Command::new(CASH)
        .arg("doctor")
        .env("PATH", &path)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let report = text(&doctor.stdout);
    assert!(
        report.contains("1 of")
            && report.contains("are not this cash.exe")
            && report.contains("wc"),
        "{report}"
    );
    assert!(report.contains("cash --link-tools"), "{report}");

    let refresh = text(&link_tools(&dir).stdout);
    assert!(refresh.contains("1 refreshed"), "{refresh}");

    let doctor = Command::new(CASH)
        .arg("doctor")
        .env("PATH", &path)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let report = text(&doctor.stdout);
    assert!(report.contains("links to this cash.exe"), "{report}");
}

#[test]
fn a_link_for_a_tool_no_longer_carried_is_removed() {
    // What a cash that dropped a tool finds: a link an older one made, still listed. A
    // listed link already gone is dropped from the manifest without a word.
    let dir = folder("retired");
    assert!(link_tools(&dir).status.success());
    let manifest = dir.join(".cash-links");
    let mut listed = std::fs::read_to_string(&manifest).unwrap();
    listed.push_str("retired-tool\nvanished-tool\n");
    std::fs::write(&manifest, listed).unwrap();
    std::fs::hard_link(CASH, dir.join("retired-tool.exe")).unwrap();

    let out = link_tools(&dir);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        stdout.contains("  removed, no longer carried: retired-tool\n"),
        "{stdout}"
    );
    assert!(!dir.join("retired-tool.exe").exists());
    let manifest = std::fs::read_to_string(&manifest).unwrap();
    let names: Vec<&str> = manifest.lines().collect();
    assert!(names.contains(&"ls"), "{manifest}");
    assert!(!names.contains(&"retired-tool"), "{manifest}");
    assert!(!names.contains(&"vanished-tool"), "{manifest}");
}

#[test]
fn the_tools_system32_also_has_are_named() {
    let dir = folder("system32");
    // A file of the user's own is not cash's link, so its name is not cash's to note.
    std::fs::write(dir.join("whoami.exe"), "not cash").unwrap();
    let out = link_tools(&dir);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));

    let named: Vec<&str> = stdout
        .split_once("  note: System32 has its own ")
        .and_then(|(_, rest)| rest.split_once(". Windows puts the user PATH after the machine's"))
        .map(|(names, _)| names.split(", ").collect())
        .unwrap_or_default();
    // Every Windows has these three.
    for tool in ["sort", "find", "timeout"] {
        assert!(named.contains(&tool), "{tool} not named: {stdout}");
    }
    let system32 = Path::new(&std::env::var_os("SystemRoot").unwrap()).join("System32");
    for tool in &named {
        assert!(
            system32.join(format!("{tool}.exe")).is_file(),
            "{tool}: {stdout}"
        );
    }
    assert!(!named.contains(&"whoami"), "{stdout}");
}

#[test]
fn which_prints_the_link_when_it_is_on_path() {
    let dir = folder("which");
    assert!(link_tools(&dir).status.success());
    let out = Command::new(CASH)
        .args(["-c", "which ls; which cd"])
        .env("PATH", path_with(&dir))
        .stdin(Stdio::null())
        .output()
        .unwrap();
    let stdout = text(&out.stdout);
    let mut lines = stdout.lines();
    let ls = lines.next().unwrap_or_default().to_lowercase();
    assert!(ls.ends_with("/link_tools/which/ls.exe"), "{stdout}");
    assert_eq!(lines.next(), Some("cd: shell builtin"), "{stdout}");
}

#[test]
fn a_folder_on_another_drive_is_an_error() {
    // Only where this machine has a second drive to try.
    let here = CASH.chars().next().unwrap_or('C').to_ascii_uppercase();
    let Some(other) = ('C'..='Z')
        .filter(|letter| *letter != here)
        .find(|letter| Path::new(&format!("{letter}:\\")).is_dir())
    else {
        return;
    };
    let dir = PathBuf::from(format!(
        "{other}:\\cash-link-tools-test-{}",
        std::process::id()
    ));
    let out = link_tools(&dir);
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out.stderr).contains("a hard link cannot cross drives"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn without_a_folder_the_links_go_in_bin_beside_cash() {
    let home = folder("default");
    let cash = home.join("cash.exe");
    std::fs::hard_link(CASH, &cash).unwrap();
    let out = Command::new(&cash)
        .arg("--link-tools")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(home.join("bin").join("ls.exe").is_file());
    assert!(home.join("bin").join(".cash-links").is_file());
}

/// Kill a process and everything it started: a linked tool re-enters its own exe for a
/// bundled tool, so `sleep.exe` is two processes.
fn kill_tree(child: &mut std::process::Child) {
    let _ = Command::new("taskkill")
        .args(["/F", "/T", "/PID", &child.id().to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    let _ = child.wait();
}

/// The files in `dir` that `--link-tools` renamed aside.
fn set_aside(dir: &Path) -> Vec<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".cash-old-"))
        .collect()
}

#[test]
fn a_link_windows_will_not_delete_is_renamed_aside_and_a_later_run_deletes_it() {
    // Links to an old cash.exe that is itself gone (`scoop cleanup` removed its version
    // folder), so the links are that file's last names, and a program outside cash still
    // runs one. Windows deletes a name of a running program while it has others, but not
    // its last: the refresh reaches that with the last link it replaces.
    let old_home = folder("in-use-old");
    let old = old_home.join("cash.exe");
    std::fs::copy(CASH, &old).unwrap();
    let dir = folder("in-use");
    let made = Command::new(&old)
        .arg("--link-tools")
        .arg(&dir)
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(made.status.success(), "{}", text(&made.stderr));
    std::fs::remove_file(&old).unwrap();
    let mut sleeping = Command::new(dir.join("sleep.exe"))
        .arg("60")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .spawn()
        .unwrap();

    let refresh = link_tools(&dir);
    let aside = set_aside(&dir);
    kill_tree(&mut sleeping);

    let stdout = text(&refresh.stdout);
    assert!(refresh.status.success(), "{}", text(&refresh.stderr));
    assert!(stdout.contains(" 0 linked, "), "{stdout}");
    assert!(stdout.contains("still in use, renamed aside"), "{stdout}");
    assert_eq!(aside.len(), 1, "{aside:?}\n{stdout}");

    // Once it has exited, the next run deletes it; Windows may take a moment to let go.
    for _ in 0..50 {
        assert!(link_tools(&dir).status.success());
        if set_aside(&dir).is_empty() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    assert!(set_aside(&dir).is_empty(), "{:?}", set_aside(&dir));
}

/// A registry key of one test's own under `HKEY_CURRENT_USER`, standing in for
/// `Environment` so that no test touches the real user `Path`; deleted when dropped.
struct UserEnvironment {
    root: String,
    key: String,
}

impl UserEnvironment {
    fn new(name: &str, path: Option<&str>) -> Self {
        let root = format!(r"Software\cash-test-{}-{name}", std::process::id());
        let key = format!(r"{root}\Environment");
        if let Some(value) = path {
            let status = Command::new("reg")
                .args(["add", &format!(r"HKCU\{key}"), "/v", "Path"])
                .args(["/t", "REG_EXPAND_SZ", "/d", value, "/f"])
                .stdout(Stdio::null())
                .status()
                .unwrap();
            assert!(status.success());
        }
        Self { root, key }
    }

    /// The stored `Path`'s type and value, as `reg query` prints them.
    fn path(&self) -> Option<(String, String)> {
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
        let (kind, value) = rest.split_once(char::is_whitespace)?;
        Some((kind.to_owned(), value.trim().to_owned()))
    }

    /// cash, pointed at this key.
    fn command(&self) -> Command {
        let mut command = Command::new(CASH);
        command
            .env("CASH_USER_ENVIRONMENT_KEY", &self.key)
            .stdin(Stdio::null());
        command
    }

    fn cash<I, S>(&self, args: I) -> Output
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.command().args(args).output().unwrap()
    }
}

impl Drop for UserEnvironment {
    fn drop(&mut self) {
        let _ = Command::new("reg")
            .args(["delete", &format!(r"HKCU\{}", self.root), "/f"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// How cash spells a folder on the user `Path`: backslashes, and no `\\?\`.
fn path_entry(dir: &Path) -> String {
    let real = std::fs::canonicalize(dir).unwrap().display().to_string();
    real.strip_prefix(r"\\?\").unwrap_or(&real).to_owned()
}

fn expand_sz(value: &str) -> (String, String) {
    ("REG_EXPAND_SZ".to_owned(), value.to_owned())
}

#[test]
fn add_to_path_puts_the_folder_first_once_and_unlink_takes_it_off() {
    let dir = folder("path");
    let before = r"%USERPROFILE%\go\bin;C:\x";
    let env = UserEnvironment::new("path", Some(before));
    let link = [
        "--link-tools".as_ref(),
        "--add-to-path".as_ref(),
        dir.as_os_str(),
    ];

    let out = env.cash(link);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        stdout.contains("added to the front of your user PATH"),
        "{stdout}"
    );
    // The type and the unexpanded %USERPROFILE% survive; the folder comes first.
    let first = format!("{};{before}", path_entry(&dir));
    assert_eq!(env.path(), Some(expand_sz(&first)));

    let again = text(&env.cash(link).stdout);
    assert!(again.contains("already on your user PATH"), "{again}");
    assert_eq!(env.path(), Some(expand_sz(&first)));

    let gone = env.cash(["--unlink-tools".as_ref(), dir.as_os_str()]);
    let stdout = text(&gone.stdout);
    assert!(gone.status.success(), "{stdout}{}", text(&gone.stderr));
    assert!(stdout.contains("taken off your user PATH"), "{stdout}");
    assert!(stdout.contains("the folder is removed"), "{stdout}");
    assert!(!stdout.contains(" 0 links removed"), "{stdout}");
    assert!(!dir.exists());
    assert_eq!(env.path(), Some(expand_sz(before)));
}

#[test]
fn a_user_with_no_path_of_their_own_is_left_with_none() {
    let dir = folder("fresh");
    let env = UserEnvironment::new("fresh", None);

    let out = env.cash([
        "--link-tools".as_ref(),
        "--add-to-path".as_ref(),
        dir.as_os_str(),
    ]);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert_eq!(env.path(), Some(expand_sz(&path_entry(&dir))));

    let gone = env.cash(["--unlink-tools".as_ref(), dir.as_os_str()]);
    assert!(gone.status.success(), "{}", text(&gone.stderr));
    assert_eq!(env.path(), None);
}

#[test]
fn without_add_to_path_it_says_how_and_writes_nothing() {
    let dir = folder("advice");
    let env = UserEnvironment::new("advice", Some(r"C:\x"));
    let out = env.cash(["--link-tools".as_ref(), dir.as_os_str()]);
    let stdout = text(&out.stdout);
    assert!(
        stdout.contains("cash --link-tools --add-to-path"),
        "{stdout}"
    );
    assert!(!stdout.contains("SetEnvironmentVariable"), "{stdout}");
    assert_eq!(env.path(), Some(expand_sz(r"C:\x")));
}

#[test]
fn a_folder_already_on_path_needs_no_advice() {
    // On the PATH cash was started with, as a terminal's own settings may put it, but not
    // on the user Path.
    let dir = folder("on-path");
    let env = UserEnvironment::new("on-path", Some(r"C:\x"));
    let out = env
        .command()
        .arg("--link-tools")
        .arg(&dir)
        .env("PATH", path_with(&dir))
        .output()
        .unwrap();
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        stdout.contains("  the folder is on PATH: other programs can run these now\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("--add-to-path"), "{stdout}");
    assert_eq!(env.path(), Some(expand_sz(r"C:\x")));
}

#[test]
fn a_folder_the_user_path_names_needs_no_advice() {
    // Named anywhere on it and spelled any way, though not on the PATH cash started with.
    let dir = folder("on-user-path");
    let entry = path_entry(&dir).replace('\\', "/").to_uppercase();
    let user_path = format!(r"C:\x;{entry}/");
    let env = UserEnvironment::new("on-user-path", Some(&user_path));
    let out = env.cash(["--link-tools".as_ref(), dir.as_os_str()]);
    let stdout = text(&out.stdout);
    assert!(out.status.success(), "{}", text(&out.stderr));
    assert!(
        stdout.contains("  the folder is on your user PATH\n"),
        "{stdout}"
    );
    assert!(!stdout.contains("--add-to-path"), "{stdout}");
    assert_eq!(env.path(), Some(expand_sz(&user_path)));
}

#[test]
fn a_bare_cash_copy_is_still_the_shell() {
    // A file named like a tool but not in a manifest is cash, not the tool.
    let dir = folder("unlisted");
    let copy = dir.join("ls.exe");
    std::fs::hard_link(CASH, &copy).unwrap();
    let out = Command::new(&copy)
        .args(["-c", "echo shell"])
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert_eq!(text(&out.stdout), "shell\n");
}
