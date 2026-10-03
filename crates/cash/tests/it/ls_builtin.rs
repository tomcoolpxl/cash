//! Native `ls` builtin tests.
//!
//! Verifies that `ls` is a native shell builtin, does NOT output placeholder
//! "somebody" / "somegroup", computes accurate hard link and directory link counts,
//! and properly handles flags (-l, -a, -A, -1, -F, -h, -d, -r, -t, -S).

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::literal_string_with_formatting_args,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::{Output, cash_command, output_of, run as cash};

struct Scratch(PathBuf);

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-ls-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create scratch dir");
        Self(dir)
    }
    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn ls_is_a_builtin() {
    let out = cash("type ls");
    assert!(
        out.stdout.contains("shell builtin"),
        "ls is not the builtin: {}",
        out.stdout
    );
}

#[test]
fn ls_resolves_dot_and_relative_paths_from_the_shell_working_directory() {
    let scratch = Scratch::new("shell-cwd");
    let child = scratch.path().join("child");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(child.join("only-here.txt"), b"marker").unwrap();

    let root = scratch.path().to_string_lossy().replace('\\', "/");
    let out = cash(&format!(
        "HOME='{root}'; cd '{root}'; cd child; ls -1; ls -1 ..; ls -1 ../child/only-here.txt; ls -1 ~"
    ));

    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "only-here.txt\nchild\n../child/only-here.txt\nchild"
    );
}

#[test]
fn ls_long_format_shows_real_user_not_somebody() {
    let scratch = Scratch::new("real-user");
    let file = scratch.path().join("alpha.txt");
    std::fs::write(&file, b"sample content").expect("write file");

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");
    let out = cash(&format!("ls -la '{dir_str}'"));

    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    // Must NOT contain the uutils non-Unix placeholders.
    assert!(
        !out.stdout.contains("somebody"),
        "stdout contains 'somebody':\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("somegroup"),
        "stdout contains 'somegroup':\n{}",
        out.stdout
    );

    // Should name the file's real owner: the current user, or on Windows Server the
    // Administrators group, which owns an elevated administrator's new files there
    // (GitHub's runners).
    let user = std::env::var("USERNAME").unwrap_or_default();
    assert!(!user.is_empty(), "USERNAME is not set");
    let row = out
        .stdout
        .lines()
        .find(|line| line.ends_with("alpha.txt"))
        .unwrap_or_else(|| panic!("no alpha.txt row:\n{}", out.stdout));
    let fields: Vec<&str> = row.split_whitespace().collect();
    assert!(
        fields
            .iter()
            .any(|field| field.eq_ignore_ascii_case(&user) || *field == "Administrators"),
        "the alpha.txt row should name its owner ({user} or Administrators): {row}"
    );

    // Starts with total <N>.
    assert!(
        out.stdout.starts_with("total "),
        "stdout should start with total: {}",
        out.stdout
    );
}

#[test]
fn ls_long_format_shows_accurate_link_counts() {
    let scratch = Scratch::new("link-counts");
    let orig = scratch.path().join("orig.txt");
    let linked = scratch.path().join("linked.txt");
    std::fs::write(&orig, b"shared bytes").expect("write orig");
    std::fs::hard_link(&orig, &linked).expect("create hard link");

    // Subdirectory with 2 child directories.
    let sub = scratch.path().join("subdir");
    let c1 = sub.join("child1");
    let c2 = sub.join("child2");
    std::fs::create_dir_all(&c1).expect("create c1");
    std::fs::create_dir_all(&c2).expect("create c2");

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");
    let out = cash(&format!("ls -l '{dir_str}'"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    // Hard linked file must show link count 2.
    let orig_line = out
        .stdout
        .lines()
        .find(|l| l.contains("orig.txt"))
        .expect("orig.txt line");
    assert!(
        orig_line.contains(" 2 "),
        "hard-linked file line should show link count 2: {orig_line}"
    );

    // Subdir with 2 children must show 2 + 2 = 4 links.
    let sub_line = out
        .stdout
        .lines()
        .find(|l| l.contains("subdir"))
        .expect("subdir line");
    assert!(
        sub_line.contains(" 4 "),
        "directory with 2 subdirs should show link count 4: {sub_line}"
    );
}

#[test]
fn ls_one_column_and_classification() {
    let scratch = Scratch::new("one-col");
    std::fs::write(scratch.path().join("doc.txt"), b"doc").unwrap();
    std::fs::write(scratch.path().join("run.exe"), b"exe").unwrap();
    std::fs::create_dir(scratch.path().join("sub")).unwrap();

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");
    let out = cash(&format!("ls -1F '{dir_str}'"));
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);

    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines.contains(&"doc.txt"));
    assert!(lines.contains(&"run.exe*"));
    assert!(lines.contains(&"sub/"));
}

#[test]
fn ls_dotfiles_filtering() {
    let scratch = Scratch::new("dotfiles");
    std::fs::write(scratch.path().join("normal.txt"), b"1").unwrap();
    std::fs::write(scratch.path().join(".hidden"), b"2").unwrap();

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");

    // Default: ignores .hidden, . and ..
    let out = cash(&format!("ls -1 '{dir_str}'"));
    assert_eq!(out.stdout.trim(), "normal.txt");

    // -A: includes .hidden, but excludes . and ..
    let out_a = cash(&format!("ls -1A '{dir_str}'"));
    let lines_a: Vec<&str> = out_a.stdout.lines().collect();
    assert_eq!(lines_a.len(), 2);
    assert!(lines_a.contains(&"normal.txt"));
    assert!(lines_a.contains(&".hidden"));
    assert!(!lines_a.contains(&"."));
    assert!(!lines_a.contains(&".."));

    // -a: includes ., .., and .hidden
    let out_all = cash(&format!("ls -1a '{dir_str}'"));
    let lines_all: Vec<&str> = out_all.stdout.lines().collect();
    assert!(lines_all.contains(&"."));
    assert!(lines_all.contains(&".."));
    assert!(lines_all.contains(&".hidden"));
    assert!(lines_all.contains(&"normal.txt"));
}

#[test]
fn ls_sorting_flags() {
    let scratch = Scratch::new("sorting");
    let small = scratch.path().join("small.txt");
    let large = scratch.path().join("large.txt");
    std::fs::write(&small, b"a").unwrap();
    std::fs::write(&large, b"a long text that is much bigger than a").unwrap();

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");

    // -S: largest first
    let out_size = cash(&format!("ls -1S '{dir_str}'"));
    let lines_size: Vec<&str> = out_size.stdout.lines().collect();
    assert_eq!(lines_size, vec!["large.txt", "small.txt"]);

    // -Sr: smallest first
    let out_size_r = cash(&format!("ls -1Sr '{dir_str}'"));
    let lines_size_r: Vec<&str> = out_size_r.stdout.lines().collect();
    assert_eq!(lines_size_r, vec!["small.txt", "large.txt"]);
}

#[test]
fn ls_directory_flag_lists_dir_itself() {
    let scratch = Scratch::new("dir-flag");
    let sub = scratch.path().join("folder");
    std::fs::create_dir(&sub).unwrap();
    std::fs::write(sub.join("file.txt"), b"inside").unwrap();

    let sub_str = sub.to_string_lossy().replace('\\', "/");
    let out = cash(&format!("ls -d '{sub_str}'"));
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout.trim(), sub_str);
}

#[test]
fn ls_human_readable_format() {
    let scratch = Scratch::new("human");
    let big = scratch.path().join("big.bin");
    // Write 2.5 MB
    let data = vec![0u8; 2_500_000];
    std::fs::write(&big, &data).unwrap();

    let dir_str = scratch.path().to_string_lossy().replace('\\', "/");
    let out = cash(&format!("ls -lh '{dir_str}'"));
    assert_eq!(out.code, 0);
    assert!(
        out.stdout.contains("2.4M") || out.stdout.contains("2.5M"),
        "expected human readable size around 2.4M-2.5M, got:\n{}",
        out.stdout
    );
}

#[test]
fn ls_nonexistent_path_returns_error() {
    let out = cash("ls /path/does/not/exist/ever");
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("No such file or directory"));
}

/// A folder of files to sort, colour and draw (D67).
fn lsd_fixture(name: &str) -> (Scratch, String) {
    let scratch = Scratch::new(name);
    let root = scratch.path();
    std::fs::create_dir_all(root.join("src/deep")).unwrap();
    std::fs::create_dir(root.join("docs")).unwrap();
    for file in [
        "b.txt",
        "a.zip",
        "c.rs",
        "file10",
        "file9",
        "file1",
        ".bashrc",
        "run.exe",
        "src/main.rs",
        "src/deep/x.txt",
    ] {
        std::fs::write(root.join(file), b"x").unwrap();
    }
    let dir = root.to_string_lossy().replace('\\', "/");
    (scratch, dir)
}

#[test]
fn ls_icons_show_when_asked_and_stay_out_of_pipes() {
    let (_scratch, dir) = lsd_fixture("icons");
    let out = cash(&format!(
        "cd '{dir}'; ls -1 --icons=always --icons-theme=fancy c.rs run.exe docs .bashrc"
    ));
    assert_eq!(out.code, 0, "{}", out.stderr);
    // lsd's glyphs: by extension, executable, folder (listed, so its contents), name.
    assert!(out.stdout.contains("\u{e68b} c.rs"), "{}", out.stdout);
    assert!(out.stdout.contains("\u{f17a} run.exe"), "{}", out.stdout);
    assert!(out.stdout.contains("\u{f1183} .bashrc"), "{}", out.stdout);

    // The colour-emoji set is gone (2026-09-28): only the Nerd glyphs, or none.
    let out = cash(&format!("cd '{dir}'; ls --icons-theme=unicode b.txt"));
    assert_eq!(out.code, 2);
    assert!(
        out.stderr.contains("Valid arguments are: 'auto', 'fancy'"),
        "{}",
        out.stderr
    );

    // `--icons` alone is `auto`: a pipe gets plain names, which scripts can use.
    let out = cash(&format!("cd '{dir}'; ls --icons b.txt | cat"));
    assert_eq!(out.stdout, "b.txt");

    let out = cash("ls --icons=maybe");
    assert_eq!(out.code, 2);
    assert!(
        out.stderr
            .contains("invalid argument 'maybe' for '--icons'"),
        "{}",
        out.stderr
    );
}

/// Run `script` as Windows Terminal starts cash in a profile whose settings are under
/// `local`, or, given `None`, outside Terminal.
fn cash_in_terminal(script: &str, local: Option<&Path>) -> Output {
    let mut command = cash_command();
    command
        .args(["-c", script])
        .env_remove("WT_SESSION")
        .env_remove("WT_PROFILE_ID");
    if let Some(local) = local {
        command
            .env("WT_SESSION", "0f6b2c1e-0000-4000-8000-000000000000")
            .env("WT_PROFILE_ID", "{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}")
            .env("LOCALAPPDATA", local);
    }
    output_of(&mut command)
}

/// Terminal's settings under `local`, the profile drawn in `face` (Terminal's own font
/// when `None`).
fn terminal_settings(local: &Path, face: Option<&str>) {
    let dir = local
        .join("Packages")
        .join("Microsoft.WindowsTerminal_8wekyb3d8bbwe")
        .join("LocalState");
    std::fs::create_dir_all(&dir).unwrap();
    let font = face.map_or_else(String::new, |face| {
        format!(r#", "font": {{ "face": "{face}" }}"#)
    });
    std::fs::write(
        dir.join("settings.json"),
        format!(
            r#"{{ "profiles": {{ "list": [ {{ "guid": "{{43e4cdd3-eb67-5e13-bd17-fa0d7f8cf3ff}}", "name": "cash"{font} }} ] }} }}"#
        ),
    )
    .unwrap();
}

fn has_nerd_glyph(text: &str) -> bool {
    text.chars()
        .any(|c| ('\u{e000}'..='\u{f8ff}').contains(&c) || c >= '\u{f0000}')
}

#[test]
fn ls_icons_are_nerd_glyphs_only_where_terminal_draws_a_nerd_font() {
    let (_scratch, dir) = lsd_fixture("icons-auto");
    let local = Scratch::new("icons-auto-terminal");
    let script = format!("cd '{dir}'; ls -1 --icons=always b.txt");

    // The tab's profile is in a Nerd Font: lsd's glyphs.
    terminal_settings(local.path(), Some("UbuntuSansMono Nerd Font Mono"));
    let out = cash_in_terminal(&script, Some(local.path()));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert!(has_nerd_glyph(&out.stdout), "{:?}", out.stdout);
    assert!(out.stdout.ends_with(" b.txt"), "{:?}", out.stdout);

    // In Terminal's own font, Cascadia Mono: no icons, rather than empty boxes.
    terminal_settings(local.path(), None);
    let out = cash_in_terminal(&script, Some(local.path()));
    assert_eq!(out.stdout, "b.txt");

    // Outside Terminal nothing says which font: no icons.
    let out = cash_in_terminal(&script, None);
    assert_eq!(out.stdout, "b.txt");

    // `fancy` asks for the glyphs wherever cash runs.
    let fancy = format!("cd '{dir}'; ls -1 --icons=always --icons-theme=fancy b.txt");
    let out = cash_in_terminal(&fancy, None);
    assert!(has_nerd_glyph(&out.stdout), "{:?}", out.stdout);
    // And `--icons=never` wins over a Nerd Font.
    terminal_settings(local.path(), Some("Hack NF"));
    let never = format!("cd '{dir}'; ls -1 --icons=never b.txt");
    assert_eq!(cash_in_terminal(&never, Some(local.path())).stdout, "b.txt");
}

#[test]
fn ls_colours_by_kind_and_extension_from_dircolors_or_ls_colors() {
    let (_scratch, dir) = lsd_fixture("colours");
    // dircolors' defaults: archives red, folders bold blue, plain files uncoloured.
    let out = cash(&format!(
        "cd '{dir}'; ls -1 -d --color=always a.zip docs b.txt"
    ));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert!(lines[0].contains("\x1b[01;31ma.zip"), "{:?}", out.stdout);
    assert_eq!(lines[1], "b.txt");
    assert!(lines[2].contains("\x1b[01;34mdocs"), "{:?}", out.stdout);

    // The shell's LS_COLORS, exported or not.
    let out = cash(&format!(
        "cd '{dir}'; LS_COLORS='*.txt=01;33'; ls -1 --color=always b.txt"
    ));
    assert!(out.stdout.contains("\x1b[01;33mb.txt"), "{:?}", out.stdout);

    let out = cash(&format!("cd '{dir}'; ls -1 --color=never a.zip"));
    assert_eq!(out.stdout, "a.zip");
}

#[test]
fn ls_sorts_by_extension_version_none_and_groups_directories() {
    let (_scratch, dir) = lsd_fixture("sorts");
    let out = cash(&format!("cd '{dir}'; ls -1 -v file*"));
    assert_eq!(out.stdout, "file1\nfile9\nfile10");
    let out = cash(&format!("cd '{dir}'; ls -1 --sort=version file10 file9"));
    assert_eq!(out.stdout, "file9\nfile10");

    // -X: names without an extension first, then by extension.
    let out = cash(&format!("cd '{dir}'; ls -1 -X b.txt c.rs a.zip file1"));
    assert_eq!(out.stdout, "file1\nc.rs\nb.txt\na.zip");

    let out = cash(&format!("cd '{dir}'; ls -1 --group-directories-first"));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(&lines[..2], ["docs", "src"], "{}", out.stdout);
    // And with -r, each group is reversed, directories still first.
    let out = cash(&format!("cd '{dir}'; ls -1 -r --group-directories-first"));
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(&lines[..2], ["src", "docs"], "{}", out.stdout);

    let out = cash(&format!("cd '{dir}'; ls -1 -U | sort | head -1"));
    assert_eq!(out.code, 0, "{}", out.stderr);

    let out = cash("ls --sort=bogus");
    assert_eq!(out.code, 2);
    assert!(
        out.stderr.contains("invalid argument 'bogus' for '--sort'"),
        "{}",
        out.stderr
    );
}

#[test]
fn ls_tree_draws_branches_to_the_depth_asked() {
    let (_scratch, dir) = lsd_fixture("tree");
    let out = cash(&format!("cd '{dir}'; ls --tree src"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "src\n├── deep\n│   └── x.txt\n└── main.rs");

    let out = cash(&format!("cd '{dir}'; ls --tree --depth=1 src"));
    assert_eq!(out.stdout, "src\n├── deep\n└── main.rs");

    // With -l, the branches sit before each name.
    let out = cash(&format!("cd '{dir}'; ls -l --tree --depth=1 src"));
    let last = out.stdout.lines().last().unwrap_or_default();
    assert!(last.ends_with(" └── main.rs"), "{}", out.stdout);

    // --depth limits -R too.
    let out = cash(&format!("cd '{dir}'; ls -R --depth=1 src"));
    assert!(!out.stdout.contains("x.txt"), "{}", out.stdout);
}

/// Sets Windows attributes on a file, as `attrib` does.
fn set_attributes(path: &Path, flags: &[&str]) {
    let status = Command::new("attrib")
        .args(flags)
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn ls_skips_hidden_system_files_unless_all_and_shows_attributes() {
    let scratch = Scratch::new("attributes");
    let root = scratch.path();
    for file in ["plain.txt", "hidden.txt", "protected.txt"] {
        std::fs::write(root.join(file), b"x").unwrap();
    }
    set_attributes(&root.join("hidden.txt"), &["+h"]);
    set_attributes(&root.join("protected.txt"), &["+h", "+s"]);
    let dir = root.to_string_lossy().replace('\\', "/");

    // Hidden and system together is what Explorer and lsd leave out; hidden alone shows.
    let out = cash(&format!("ls -1 '{dir}'"));
    assert_eq!(out.stdout, "hidden.txt\nplain.txt", "{}", out.stderr);
    let out = cash(&format!("ls -1A '{dir}'"));
    assert_eq!(out.stdout, "hidden.txt\nplain.txt\nprotected.txt");
    // Named on the command line, it is listed.
    let out = cash(&format!("ls -1 '{dir}/protected.txt'"));
    assert!(out.stdout.ends_with("protected.txt"), "{}", out.stdout);

    let out = cash(&format!("cd '{dir}'; ls -1A --attributes"));
    assert_eq!(
        out.stdout, ".a-h- hidden.txt\n.a--- plain.txt\n.a-hs protected.txt",
        "{}",
        out.stderr
    );
    let out = cash(&format!("cd '{dir}'; ls -lA --attributes protected.txt"));
    assert!(
        out.stdout.starts_with("-rw-rw-r-- .a-hs "),
        "{}",
        out.stdout
    );

    set_attributes(&root.join("hidden.txt"), &["-h"]);
    set_attributes(&root.join("protected.txt"), &["-h", "-s"]);
}

#[test]
fn ls_long_w_is_the_access_list_answer_not_just_read_only() {
    let scratch = Scratch::new("acl-write");
    let file = scratch.path().join("locked.txt");
    std::fs::write(&file, b"x").unwrap();
    let dir = scratch.path().to_string_lossy().replace('\\', "/");

    let out = cash(&format!("ls -l '{dir}/locked.txt'"));
    assert!(out.stdout.starts_with("-rw-rw-r--"), "{}", out.stdout);

    // Deny this user writing the file's data, as an access list can without the
    // read-only attribute.
    let user = std::env::var("USERNAME").unwrap();
    let deny = Command::new("icacls")
        .arg(&file)
        .args(["/deny", &format!("{user}:(WD)")])
        .output()
        .unwrap();
    assert!(
        deny.status.success(),
        "{}",
        String::from_utf8_lossy(&deny.stderr)
    );

    let out = cash(&format!("ls -l '{dir}/locked.txt'"));
    let _ = Command::new("icacls")
        .arg(&file)
        .args(["/remove:d", &user])
        .output();
    assert!(out.stdout.starts_with("-r--r--r--"), "{}", out.stdout);

    // A read-only folder is only a customised one: it still takes new files.
    let folder = scratch.path().join("custom");
    std::fs::create_dir(&folder).unwrap();
    set_attributes(&folder, &["+r"]);
    let out = cash(&format!("ls -ld '{dir}/custom'"));
    assert!(out.stdout.starts_with("drwxrwxr-x"), "{}", out.stdout);
    set_attributes(&folder, &["-r"]);
}

#[test]
fn ls_long_colours_every_column_when_colour_is_on() {
    let scratch = Scratch::new("column-colours");
    std::fs::write(scratch.path().join("f.txt"), b"x").unwrap();
    let dir = scratch.path().to_string_lossy().replace('\\', "/");

    let out = cash(&format!("ls -l --color=always '{dir}/f.txt'"));
    assert_eq!(out.code, 0, "{}", out.stderr);
    // lsd's scheme: r yellow, w red, - grey; the owner (this user) pale yellow; a small
    // size pale; a date from the last hour bright green. An elevated process's files
    // belong to Administrators, not the user, as on GitHub's runners: grey, like any
    // other owner.
    assert!(
        out.stdout
            .starts_with("\x1b[90m-\x1b[0m\x1b[33mr\x1b[0m\x1b[31mw\x1b[0m"),
        "{:?}",
        out.stdout
    );
    let owner = if cash("echo $EUID").stdout == "0" {
        "\x1b[38;5;245mAdministrators"
    } else {
        "\x1b[38;5;230m"
    };
    assert!(out.stdout.contains(owner), "{:?}", out.stdout);
    assert!(
        out.stdout.contains("\x1b[38;5;229m1\x1b[0m"),
        "{:?}",
        out.stdout
    );
    assert!(out.stdout.contains("\x1b[38;5;40m"), "{:?}", out.stdout);

    // Off means off, for every column.
    let out = cash(&format!("ls -l --color=never '{dir}/f.txt'"));
    assert!(!out.stdout.contains('\x1b'), "{:?}", out.stdout);
    let out = cash(&format!("ls -l '{dir}/f.txt' | cat"));
    assert!(!out.stdout.contains('\x1b'), "{:?}", out.stdout);
}
