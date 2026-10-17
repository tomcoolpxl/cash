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

use std::path::Path;
use std::process::Command;

use crate::common::{Output, Scratch, cash_command, output_of, run as cash};

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

    // A folder shows 1, as Git Bash shows it, however many subfolders it has: Linux's
    // 2 + subfolders took reading every folder listed, and `..` of a temporary folder
    // held 160,000 of them.
    let sub_line = out
        .stdout
        .lines()
        .find(|l| l.contains("subdir"))
        .expect("subdir line");
    assert!(
        sub_line.contains(" 1 "),
        "a folder should show link count 1: {sub_line}"
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
fn a_repeated_option_overrides_the_one_before() {
    // As in GNU ls and Bash's builtins: `alias ls='ls --color=auto'` and then
    // `ls --color=never` gives `--color` twice, which clap refused ("cannot be used
    // multiple times", status 2). The rest of the builtins parsed by clap share it.
    let scratch = Scratch::new("repeated");
    std::fs::create_dir(scratch.path().join("sub")).unwrap();
    let dir = scratch.as_script_path();
    let out = cash(&format!(
        "cd '{dir}' && ls -F -F && ls --color=always --color=never && \
         shopt -s expand_aliases && alias ls='ls --color=always'
         ls --color=never; tree -a -a --noreport ."
    ));
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str(), out.code),
        ("sub/\nsub\nsub\n.\n└── sub", "", 0)
    );
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
    assert_eq!(out.code, 1);
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
    assert_eq!(out.code, 1);
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

/// `ls` whose reader went away ends in silence with 141, as SIGPIPE would end it (D71).
/// It said "cannot open directory '…': The pipe is being closed." and ended with 2
/// (BI-14).
#[test]
fn ls_ends_with_141_when_its_reader_goes() {
    let out = cash(r#"ls -1 C:/Windows/System32 | head -1 >/dev/null; echo "${PIPESTATUS[*]}""#);
    assert_eq!(out.stdout, "141 0", "{}", out.stderr);
    assert!(out.stderr.is_empty(), "{}", out.stderr);
}

/// Files named beside a folder are listed first, then one blank line and the folder's
/// heading, as GNU `ls` does. cash printed two blank lines there, with `-l`, `-R` and
/// `--tree` too, and after an operand that does not exist.
#[test]
fn files_and_then_a_folder_are_one_blank_line_apart() {
    let scratch = Scratch::new("ls-files-then-folder");
    std::fs::write(scratch.join("s1"), "").unwrap();
    std::fs::create_dir(scratch.join("sub")).unwrap();
    std::fs::write(scratch.join("sub").join("inner"), "").unwrap();
    let ls = |args: &str| {
        output_of(
            cash_command()
                .args(["-c", &format!("ls {args}")])
                .current_dir(scratch.path()),
        )
    };

    assert_eq!(ls("s1 sub").stdout, "s1\n\nsub:\ninner");
    assert_eq!(ls("-R s1 sub").stdout, "s1\n\nsub:\ninner");
    assert_eq!(ls("--tree s1 sub").stdout, "s1\n\nsub\n└── inner");
    let missing = ls("nope s1 sub");
    assert_eq!(missing.stdout, "s1\n\nsub:\ninner");
    assert_eq!(missing.code, 2);
    let long = ls("-l s1 sub").stdout;
    let lines: Vec<&str> = long.lines().collect();
    assert_eq!(
        lines.get(1..4),
        Some(&["", "sub:", "total 0"][..]),
        "{long}"
    );
    // Folders alone were right already: one blank line between them.
    std::fs::create_dir(scratch.join("sub2")).unwrap();
    assert_eq!(ls("sub sub2").stdout, "sub:\ninner\n\nsub2:");
}

/// `ls -R` reports a directory below it that it cannot open, prints no header for it and
/// exits 1, as GNU `ls` does. It printed the header with nothing under it, said
/// nothing, and exited 0 (BI-14).
#[test]
fn ls_r_reports_a_directory_it_cannot_open() {
    let scratch = Scratch::new("ls-r-shut");
    for dir in ["open", "shut"] {
        std::fs::create_dir(scratch.join(dir)).unwrap();
        std::fs::write(scratch.join(dir).join("f"), "").unwrap();
    }
    let user = std::env::var("USERNAME").unwrap();
    let icacls = |args: &[&str]| {
        let status = Command::new("icacls")
            .arg(scratch.join("shut"))
            .args(args)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "icacls {args:?}");
    };
    icacls(&["/deny", &format!("{user}:(RX)")]);
    let out = output_of(
        cash_command()
            .args(["-c", "ls -R ."])
            .current_dir(scratch.path()),
    );
    icacls(&["/remove:d", &user]);

    assert_eq!(out.stdout, ".:\nopen\nshut\n\n./open:\nf");
    assert_eq!(
        out.stderr,
        "ls: cannot open directory './shut': Permission denied"
    );
    assert_eq!(out.code, 1);
}

/// `ls -R foo/` joins the names below once, as GNU `ls` does: it printed `foo//bar:`.
#[test]
fn ls_r_joins_names_below_a_trailing_slash_once() {
    let scratch = Scratch::new("ls-r-slash");
    std::fs::create_dir_all(scratch.join("foo").join("bar")).unwrap();
    std::fs::write(scratch.join("foo").join("bar").join("f"), "").unwrap();
    let out = output_of(
        cash_command()
            .args(["-c", "ls -R foo/"])
            .current_dir(scratch.path()),
    );
    assert_eq!(out.stdout, "foo/:\nbar\n\nfoo/bar:\nf");
    assert_eq!(out.code, 0, "{}", out.stderr);
}

/// `ls --tree` says a folder below it that it cannot open, as `-R` says it, status 1: it
/// drew the folder with nothing under it, said nothing and exited 0.
#[test]
fn ls_tree_reports_a_directory_it_cannot_open() {
    let scratch = Scratch::new("ls-tree-shut");
    for dir in ["open", "shut"] {
        std::fs::create_dir(scratch.join(dir)).unwrap();
        std::fs::write(scratch.join(dir).join("f"), "").unwrap();
    }
    let user = std::env::var("USERNAME").unwrap();
    let icacls = |args: &[&str]| {
        let status = Command::new("icacls")
            .arg(scratch.join("shut"))
            .args(args)
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "icacls {args:?}");
    };
    icacls(&["/deny", &format!("{user}:(RX)")]);
    let out = output_of(
        cash_command()
            .args(["-c", "ls --tree ."])
            .current_dir(scratch.path()),
    );
    icacls(&["/remove:d", &user]);

    assert_eq!(out.stdout, ".\n├── open\n│   └── f\n└── shut");
    assert_eq!(
        out.stderr,
        "ls: cannot open directory './shut': Permission denied"
    );
    assert_eq!(out.code, 1);
}

/// A tree of names to lay out, as GNU ls's options were checked on in Git Bash.
fn layout_fixture(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    for dir in ["dir1", "sub"] {
        std::fs::create_dir(scratch.join(dir)).unwrap();
    }
    for file in [
        "a.txt",
        "b~",
        ".hidden",
        "c d",
        "e.sh",
        "zz.log",
        "another-long-file-name.txt",
        "yet-another-long-name.md",
    ] {
        std::fs::write(scratch.join(file), "x").unwrap();
    }
    scratch
}

/// `script` in `dir`, 60 columns wide and in UTC, as the expectations were taken.
fn ls_in(dir: &Path, script: &str) -> Output {
    output_of(
        cash_command()
            .args(["-c", script])
            .current_dir(dir)
            .env("COLUMNS", "60")
            .env("TZ", "UTC"),
    )
}

/// `-x`, `-C` and `-m` lay names out as GNU ls does in Git Bash, each column as wide as
/// its widest name (spaces where GNU writes tabs), and `-p` marks folders.
#[test]
fn ls_layouts_fill_the_width_as_gnu_ls_does() {
    let scratch = layout_fixture("ls-layouts");
    let out = ls_in(
        scratch.path(),
        "ls -x; echo; ls -C; echo; ls -m; echo; ls -w 40 -x; echo; ls --format=commas -p; \
         echo; ls -p",
    );
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(
        out.stdout,
        "a.txt  another-long-file-name.txt  b~      c d  dir1  e.sh\n\
         sub    yet-another-long-name.md    zz.log\n\
         \n\
         a.txt                       c d   sub\n\
         another-long-file-name.txt  dir1  yet-another-long-name.md\n\
         b~                          e.sh  zz.log\n\
         \n\
         a.txt, another-long-file-name.txt, b~, c d, dir1, e.sh, sub,\n\
         yet-another-long-name.md, zz.log\n\
         \n\
         a.txt   another-long-file-name.txt\n\
         b~      c d\n\
         dir1    e.sh\n\
         sub     yet-another-long-name.md\n\
         zz.log\n\
         \n\
         a.txt, another-long-file-name.txt, b~, c d, dir1/, e.sh,\n\
         sub/, yet-another-long-name.md, zz.log\n\
         \n\
         a.txt\nanother-long-file-name.txt\nb~\nc d\ndir1/\ne.sh\nsub/\nyet-another-long-name.md\nzz.log"
    );
}

/// `-I` and `-B` leave names out however `-a` asks, `--hide` only without `-a` or `-A`.
#[test]
fn ls_leaves_out_what_ignore_hide_and_backups_name() {
    let scratch = layout_fixture("ls-ignore");
    let line = |script: &str| {
        let out = ls_in(scratch.path(), &format!("{script} | tr '\\n' ' '"));
        assert_eq!(out.code, 0, "{script}: {}", out.stderr);
        out.stdout
    };
    assert_eq!(
        line("ls -I '*.txt'"),
        "b~ c d dir1 e.sh sub yet-another-long-name.md zz.log"
    );
    assert_eq!(
        line("ls -a --ignore='*.txt'"),
        ". .. .hidden b~ c d dir1 e.sh sub yet-another-long-name.md zz.log"
    );
    assert_eq!(
        line("ls --hide='*.txt'"),
        "b~ c d dir1 e.sh sub yet-another-long-name.md zz.log"
    );
    assert_eq!(
        line("ls -A --hide='*.txt'"),
        ".hidden a.txt another-long-file-name.txt b~ c d dir1 e.sh sub \
         yet-another-long-name.md zz.log"
    );
    assert_eq!(
        line("ls -aB"),
        ". .. .hidden a.txt another-long-file-name.txt c d dir1 e.sh sub \
         yet-another-long-name.md zz.log"
    );
    assert_eq!(
        line("ls --hide='*.txt' --hide='*.md'"),
        "b~ c d dir1 e.sh sub zz.log"
    );
}

/// The long format's options and times, as GNU ls shows them: `-o` leaves out the group,
/// `-g` the owner; `--full-time` and each `--time-style`, in `TZ`'s zone.
#[test]
fn ls_long_options_and_time_styles_as_gnu_ls_has_them() {
    let scratch = Scratch::new("ls-times");
    std::fs::write(scratch.join("a.txt"), "abc").unwrap();
    std::fs::write(scratch.join("old.txt"), "").unwrap();
    let at = |name: &str, seconds: u64, nanos: u32| {
        let file = std::fs::File::options()
            .write(true)
            .open(scratch.join(name))
            .unwrap();
        file.set_modified(std::time::UNIX_EPOCH + std::time::Duration::new(seconds, nanos))
            .unwrap();
    };
    at("a.txt", 1_767_319_445, 123_456_700);
    at("old.txt", 1_741_080_030, 0);
    let out = ls_in(
        scratch.path(),
        "ls -l | awk 'NR > 1 {print NF}'; ls -o | awk 'NR > 1 {print NF}'; \
         ls -og | awk 'NR > 1 {print NF}'; \
         for style in full-iso long-iso iso locale +%Y/%m/%d posix-long-iso; do \
           ls -o --time-style=$style a.txt | cut -d' ' -f4-; done; \
         ls --full-time old.txt | cut -d' ' -f6-; \
         TIME_STYLE=long-iso ls -o a.txt | cut -d' ' -f4-; \
         ls --time-style=bogus a.txt; echo \"rc=$?\"",
    );
    assert_eq!(
        out.stdout,
        "9\n9\n8\n8\n7\n7\n\
         3 2026-01-02 02:04:05.123456700 +0000 a.txt\n\
         3 2026-01-02 02:04 a.txt\n\
         3 2026-01-02  a.txt\n\
         3 Jan  2  2026 a.txt\n\
         3 2026/01/02 a.txt\n\
         3 Jan  2  2026 a.txt\n\
         2025-03-04 09:20:30.000000000 +0000 old.txt\n\
         3 2026-01-02 02:04 a.txt\n\
         a.txt\n\
         rc=0",
        "{}",
        out.stderr
    );
    let out = ls_in(scratch.path(), "ls -l --time-style=bogus");
    assert_eq!(out.code, 2);
    assert_eq!(
        out.stderr,
        "ls: invalid argument 'bogus' for 'time style'\n\
         Valid arguments are:\n  - [posix-]full-iso\n  - [posix-]long-iso\n  - [posix-]iso\n  \
         - [posix-]locale\n  - +FORMAT (e.g., +%H:%M) for a 'date'-style format\n\
         Try 'ls --help' for more information."
    );
}

/// A word that is none of `--format`'s or `--indicator-style`'s ends `ls` with GNU's list
/// and status 1; a width that is no number with status 2.
#[test]
fn ls_refuses_a_bad_value_as_gnu_ls_does() {
    let scratch = Scratch::new("ls-bad-values");
    let out = ls_in(scratch.path(), "ls --format=bogus");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "ls: invalid argument 'bogus' for '--format'\n\
         Valid arguments are:\n  - 'verbose', 'long'\n  - 'commas'\n  - 'horizontal', 'across'\n  \
         - 'vertical'\n  - 'single-column'\n\
         Try 'ls --help' for more information."
    );
    let out = ls_in(scratch.path(), "ls --indicator-style=bogus");
    assert_eq!(out.code, 1, "{}", out.stderr);
    // `--color` and `--classify` list the words that mean the same on a line, as GNU's
    // argmatch does, and end with 1: they said one line of three words and ended with 2.
    for option in ["color", "classify"] {
        let out = ls_in(scratch.path(), &format!("ls --{option}=bogus"));
        assert_eq!(out.code, 1, "{option}");
        assert_eq!(
            out.stderr,
            format!(
                "ls: invalid argument 'bogus' for '--{option}'\n\
                 Valid arguments are:\n  - 'always', 'yes', 'force'\n  - 'never', 'no', 'none'\n  \
                 - 'auto', 'tty', 'if-tty'\n\
                 Try 'ls --help' for more information."
            )
        );
    }
    let out = ls_in(scratch.path(), "ls --sort=bogus");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr
            .contains("Valid arguments are:\n  - 'none'\n  - 'name'\n  - 'time'\n"),
        "{}",
        out.stderr
    );
    let out = ls_in(scratch.path(), "ls -w abc");
    assert_eq!(out.code, 2);
    assert_eq!(out.stderr, "ls: invalid line width: 'abc'");
}

/// `-L` shows what a junction names, and with `-R` goes into it, but not round a junction
/// back to a folder above: said as GNU ls says it, status 2.
#[test]
fn ls_l_shows_and_walks_what_a_link_names() {
    let scratch = Scratch::new("ls-dereference");
    std::fs::create_dir_all(scratch.join("d").join("sub")).unwrap();
    std::fs::create_dir(scratch.join("real")).unwrap();
    std::fs::write(scratch.join("real").join("f"), "x").unwrap();
    cash_win32::junction::point(
        &scratch.join("d").join("sub").join("up"),
        &scratch.join("d"),
    )
    .unwrap();
    cash_win32::junction::point(&scratch.join("j"), &scratch.join("real")).unwrap();
    let out = ls_in(
        scratch.path(),
        "ls -l j | cut -c1; ls -lL -d j | cut -c1; ls -RL d; echo \"rc=$?\"",
    );
    assert_eq!(
        out.stdout, "l\nd\nd:\nsub\n\nd/sub:\nup\nrc=2",
        "{}",
        out.stderr
    );
    assert_eq!(
        out.stderr,
        "ls: d/sub/up: not listing already-listed directory"
    );
}

/// `help ls` gives `ls --help`'s options under its usage, the new ones among them: it gave
/// clap's page of the few options clap knew.
#[test]
fn help_ls_gives_ls_help_options() {
    let out = cash("help ls | grep -c -- '--time-style=STYLE\\|-L, --dereference'");
    assert_eq!(out.stdout, "2", "{}", out.stderr);
}

/// Four files: modified in 2020, read in 2021, made now; `plain`, one byte, then made
/// read-only, which only its change time shows.
fn sizes_and_times_fixture(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    std::fs::write(scratch.join("plain"), "x").unwrap();
    std::fs::write(scratch.join("big"), vec![0u8; 5000]).unwrap();
    std::fs::write(scratch.join("mega"), vec![0u8; 1_000_000]).unwrap();
    std::fs::write(scratch.join("m2"), vec![0u8; 2_000_000]).unwrap();
    let modified = std::time::UNIX_EPOCH + std::time::Duration::from_mins(26_500_000);
    let accessed = std::time::UNIX_EPOCH + std::time::Duration::from_hours(450_000);
    for name in ["plain", "big", "mega", "m2"] {
        let file = std::fs::File::options()
            .write(true)
            .open(scratch.join(name))
            .unwrap();
        file.set_times(
            std::fs::FileTimes::new()
                .set_modified(modified)
                .set_accessed(accessed),
        )
        .unwrap();
    }
    // NTFS stamps a change with the clock's tick, as coarse as 15.6 ms: the read-only
    // change must come a tick after the times set above, or the change times tie.
    std::thread::sleep(std::time::Duration::from_millis(50));
    let mut readonly = std::fs::metadata(scratch.join("plain"))
        .unwrap()
        .permissions();
    readonly.set_readonly(true);
    std::fs::set_permissions(scratch.join("plain"), readonly).unwrap();
    scratch
}

/// Makes the fixture's `plain` writable again, so that the scratch folder can go.
fn writable_again(scratch: &Scratch) {
    let mut writable = std::fs::metadata(scratch.join("plain"))
        .unwrap()
        .permissions();
    #[expect(
        clippy::permissions_set_readonly_false,
        reason = "the scratch folder is removed after the test, its files with it"
    )]
    writable.set_readonly(false);
    std::fs::set_permissions(scratch.join("plain"), writable).unwrap();
}

/// `-s`, `-k`, `--block-size`, `--si` and `-h` as GNU ls has them, with the blocks the
/// volume gives a file (NTFS's 4 KiB clusters, counted in whole KiB as Git Bash shows
/// them) and the totals counted the same way: checked against Git Bash's ls (coreutils
/// 8.32) and GNU coreutils 9.10. `-h` rounds up, as GNU does: 2,000,000 bytes read 1.9M.
#[test]
fn ls_sizes_and_blocks_as_gnu_ls_has_them() {
    let scratch = sizes_and_times_fixture("ls-sizes");
    let out = ls_in(
        scratch.path(),
        "ls -s plain big mega; ls -sk big; ls -s --block-size=1 plain big mega; \
         ls -s --block-size=K plain big mega; ls -s --block-size=KB plain big mega; \
         ls -sh plain big mega; ls -s --si big mega; ls -sm plain big; \
         ls -lh big mega m2 | awk '{print $5}'; ls -l --si mega m2 | awk '{print $5}'; \
         ls -l --block-size=K plain big | awk '{print $5}'; ls -l | head -1; ls -s | head -1",
    );
    assert_eq!(
        out.stdout,
        [
            "  8 big",
            "980 mega",
            "  1 plain",
            "8 big",
            "   8192 big",
            "1003520 mega",
            "   1024 plain",
            "  8K big",
            "980K mega",
            "  1K plain",
            "   9kB big",
            "1004kB mega",
            "   2kB plain",
            "8.0K big",
            "980K mega",
            "1.0K plain",
            "8.2k big",
            "1.1M mega",
            "8 big, 1 plain",
            "4.9K",
            "2.0M",
            "977K",
            "2.0M",
            "1.0M",
            "5K",
            "1K",
            "total 2945",
            "total 2945",
        ]
        .join("\n"),
        "{}",
        out.stderr
    );
    for (spec, message) in [
        ("bogus", "ls: invalid --block-size argument 'bogus'"),
        ("1Q", "ls: invalid suffix in --block-size argument '1Q'"),
    ] {
        let out = ls_in(scratch.path(), &format!("ls --block-size={spec}"));
        assert_eq!((out.code, out.stderr.as_str()), (2, message));
    }
    writable_again(&scratch);
}

/// `-c` (NTFS's change time, as Git Bash shows it), `-u` and `--time`: the time `-l`
/// shows and `-t` sorts by, and without `-l` the order, as GNU ls has them.
#[test]
fn ls_times_as_gnu_ls_has_them() {
    let scratch = sizes_and_times_fixture("ls-times-kinds");
    let out = ls_in(
        scratch.path(),
        "ls -l --time-style=+%Y plain | awk '{print $6}'; \
         ls -lu --time-style=+%Y plain | awk '{print $6}'; \
         ls -l --time=birth --time-style=+%Y plain | awk '{print $6}'; \
         ls -lc --time-style=+%Y plain | awk '{print $6}'; \
         ls -1 -c; ls -lc | awk 'NR > 1 {print $9}'",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.get(..2),
        Some(&["2020", "2021"][..]),
        "{}",
        out.stdout
    );
    // Birth and the change are now, whatever year that is.
    assert!(
        lines.get(2).is_some_and(|year| *year > "2025"),
        "{}",
        out.stdout
    );
    assert_eq!(lines.get(2), lines.get(3), "{}", out.stdout);
    // Without -l, -c sorts by the change time, the read-only one last changed first;
    // with -l it sorts by name.
    assert_eq!(lines.get(4), Some(&"plain"), "{}", out.stdout);
    assert_eq!(
        lines.get(8..),
        Some(&["big", "m2", "mega", "plain"][..]),
        "{}",
        out.stdout
    );
    let out = ls_in(scratch.path(), "ls --time=bogus");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "ls: invalid argument 'bogus' for '--time'\n\
         Valid arguments are:\n  - 'atime', 'access', 'use'\n  - 'ctime', 'status'\n  \
         - 'mtime', 'modification'\n  - 'birth', 'creation'\n\
         Try 'ls --help' for more information."
    );
    writable_again(&scratch);
}

/// Names to quote: a space, a quote, DEL (a control character Windows allows in a
/// name), a folder with a space, and a link to it.
fn quoting_fixture(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    for name in ["plain", "a b", "it's", "c\u{7f}d"] {
        std::fs::write(scratch.join(name), "").unwrap();
    }
    std::fs::create_dir(scratch.join("sub dir")).unwrap();
    std::fs::write(scratch.join("sub dir").join("inner"), "").unwrap();
    cash_win32::junction::point(&scratch.join("link"), &scratch.join("sub dir")).unwrap();
    scratch
}

/// `--quoting-style`, `-N`, `-b`, `-Q`, `-q` and `QUOTING_STYLE` quote names as GNU ls
/// 9.10 does, the last of them given winning. Into a pipe, names are as they are unless
/// asked.
#[test]
fn ls_quotes_names_as_gnu_ls_does() {
    let scratch = quoting_fixture("ls-quoting");
    let out = ls_in(
        scratch.path(),
        "ls -1; ls -1 --quoting-style=shell-escape; ls -1q; ls -1b; ls -1 -Q -N; ls -1 -N -Q; \
         ls -1 --quoting-style=locale -d 'a b'; QUOTING_STYLE=c ls -1d plain; \
         QUOTING_STYLE=c ls -1d -N plain",
    );
    assert_eq!(
        out.stdout,
        [
            "a b",
            "c\u{7f}d",
            "it's",
            "link",
            "plain",
            "sub dir",
            "'a b'",
            "'c'$'\\177''d'",
            "\"it's\"",
            "link",
            "plain",
            "'sub dir'",
            "a b",
            "c?d",
            "it's",
            "link",
            "plain",
            "sub dir",
            "a\\ b",
            "c\\177d",
            "it's",
            "link",
            "plain",
            "sub\\ dir",
            "a b",
            "c\u{7f}d",
            "it's",
            "link",
            "plain",
            "sub dir",
            "\"a b\"",
            "\"c\\177d\"",
            "\"it's\"",
            "\"link\"",
            "\"plain\"",
            "\"sub dir\"",
            "\u{2018}a b\u{2019}",
            "\"plain\"",
            "plain",
        ]
        .join("\n"),
        "{}",
        out.stderr
    );
}

/// Names that need no quotes get a space before them in `-l` and `-C` where others are
/// quoted, in the shell styles and c-maybe and not without a width, as GNU ls 9.10 lines
/// them up; headers and link targets are quoted too, and a style or a `QUOTING_STYLE` GNU
/// does not know is refused or passed over as GNU's are.
#[test]
fn ls_lines_up_quoted_names_as_gnu_ls_does() {
    let scratch = quoting_fixture("ls-quoting-lined-up");
    let out = ls_in(
        scratch.path(),
        "ls -l --time-style=+T --quoting-style=shell | sed -n 's/^.* T //p'; \
         ls -Cd --quoting-style=shell -- 'a b' \"it's\" plain 'sub dir'; \
         ls -R --quoting-style=shell 'sub dir'; \
         ls -Cd -w 0 --quoting-style=shell -- 'a b' plain; \
         ls -Cd --quoting-style=c-maybe -- c* plain",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.get(..3),
        Some(&["'a b'", " c\u{7f}d", "\"it's\""][..]),
        "{}",
        out.stdout
    );
    assert!(
        lines
            .get(3)
            .is_some_and(|line| line.starts_with(" link -> '") && line.ends_with("sub dir'")),
        "{}",
        out.stdout
    );
    assert_eq!(
        lines.get(4..),
        Some(
            &[
                " plain",
                "'sub dir'",
                "'a b'  \"it's\"   plain  'sub dir'",
                "'sub dir':",
                "inner",
                // Without a width GNU lines nothing up; c-maybe lines up as the shell
                // styles do.
                "'a b'  plain",
                "\"c\\177d\"   plain",
            ][..]
        ),
        "{}",
        out.stdout
    );

    let out = ls_in(scratch.path(), "QUOTING_STYLE=bogus ls -d plain");
    assert_eq!(
        (out.stdout.as_str(), out.stderr.as_str()),
        (
            "plain",
            "ls: ignoring invalid value of environment variable QUOTING_STYLE: 'bogus'"
        )
    );
    let out = ls_in(scratch.path(), "ls --quoting-style=bogus");
    assert_eq!(out.code, 1);
    assert_eq!(
        out.stderr,
        "ls: invalid argument 'bogus' for '--quoting-style'\n\
         Valid arguments are:\n  - 'literal'\n  - 'shell'\n  - 'shell-always'\n  \
         - 'shell-escape'\n  - 'shell-escape-always'\n  - 'c'\n  - 'c-maybe'\n  - 'escape'\n  \
         - 'locale'\n  - 'clocale'\n\
         Try 'ls --help' for more information."
    );
}

/// A file, a name with a space, a folder, and a junction to it, for the script helpers.
fn helpers_fixture(name: &str) -> Scratch {
    let scratch = Scratch::new(name);
    std::fs::write(scratch.join("plain"), "").unwrap();
    std::fs::write(scratch.join("a b"), "").unwrap();
    std::fs::create_dir(scratch.join("sub")).unwrap();
    std::fs::write(scratch.join("sub").join("inner"), "").unwrap();
    cash_win32::junction::point(&scratch.join("linkdir"), &scratch.join("sub")).unwrap();
    scratch
}

/// `--zero`, `-i`, `-H`, `--dereference-command-line-symlink-to-dir` and `-T` as GNU ls
/// 9.10 has them: NUL after each line of entries, never after a header or between
/// folders; the file index Git Bash's `ls -i` and `stat` show; links named on the command
/// line followed; a tab size checked.
#[test]
fn ls_script_helpers_as_gnu_ls_has_them() {
    let scratch = helpers_fixture("ls-helpers");
    let out = ls_in(
        scratch.path(),
        "ls --zero; echo; ls --zero sub plain; echo; ls --zero -m; echo; \
         ls --zero --quoting-style=shell -d 'a b'; echo; ls -1 --zero -C -d plain sub",
    );
    assert_eq!(
        out.stdout,
        "a b\0linkdir\0plain\0sub\0\nplain\0\nsub:\ninner\0\na b, linkdir, plain, sub\0\n\
         'a b'\0\nplain  sub\0",
        "{}",
        out.stderr
    );

    let out = ls_in(
        scratch.path(),
        "ls -i1 plain; stat -c %i plain; ls -1H linkdir; \
         ls -1lH --time-style=+T linkdir | sed -n 's/^.* T //p'; \
         ls -1l --time-style=+T linkdir | cut -c1; ls -1dlH --time-style=+T linkdir | cut -c1; \
         ls -1l --dereference-command-line-symlink-to-dir --time-style=+T linkdir \
         | sed -n 's/^.* T //p'; ls -1F linkdir; ls -Cd -T 4 -w 40 plain sub",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    let index = lines.get(1).copied().unwrap_or_default();
    assert!(
        index.parse::<u64>().is_ok_and(|index| index > 0),
        "{}",
        out.stdout
    );
    assert_eq!(lines.first().copied(), Some(&*format!("{index} plain")));
    assert_eq!(
        lines.get(2..),
        Some(
            &[
                "inner",
                "inner",
                "l",
                "d",
                "inner",
                "linkdir@",
                "plain  sub"
            ][..]
        ),
        "{}",
        out.stderr
    );

    let out = ls_in(
        scratch.path(),
        "ls -T x; echo $?; ls -T -1; ls -T 99999999999999999999; echo $?",
    );
    assert_eq!(out.stdout, "2\n2", "{}", out.stderr);
    assert_eq!(
        out.stderr,
        "ls: invalid tab size: 'x'\nls: invalid tab size: '-1'\n\
         ls: invalid tab size: '99999999999999999999': Value too large for defined data type"
    );
}

/// `--hyperlink` makes each name an OSC 8 link to its file, as GNU ls 9.10 does, with no
/// host in the URL, which Windows Terminal opens (GNU names the machine): headers too,
/// a link's name and target to what it resolves to, and the quotes of a quoted name
/// outside the link where names line up.
#[test]
fn ls_hyperlinks_names_as_gnu_ls_does() {
    let scratch = helpers_fixture("ls-hyperlinks");
    let link = |text: &str| format!("\x1b\\{text}\x1b]8;;\x1b\\");
    let out = ls_in(
        scratch.path(),
        "ls -1 --hyperlink 'a b'; ls --hyperlink=always sub plain; \
         ls -l --time-style=+T --hyperlink=always --quoting-style=shell 'a b' plain \
         | sed -n 's/^.* T //p'; ls -ld --time-style=+T --hyperlink=always linkdir \
         | sed -n 's/^.* T //p'; ls --hyperlink=auto plain; ls --hyperlink=never plain",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    let starts = |line: usize, start: &str, end: &str| {
        lines
            .get(line)
            .is_some_and(|text| text.starts_with(start) && text.ends_with(end))
    };
    let url = "\x1b]8;;file:///";
    assert!(
        starts(0, url, &format!("/a%20b{}", link("a b"))),
        "{lines:?}"
    );
    assert!(
        starts(1, url, &format!("/plain{}", link("plain"))),
        "{lines:?}"
    );
    assert_eq!(lines.get(2), Some(&""), "{lines:?}");
    assert!(
        starts(3, url, &format!("/sub{}:", link("sub"))),
        "{lines:?}"
    );
    assert!(
        starts(4, url, &format!("/sub/inner{}", link("inner"))),
        "{lines:?}"
    );
    assert!(
        starts(5, &format!("'{url}"), &format!("{}'", link("a b"))),
        "{lines:?}"
    );
    assert!(starts(6, &format!(" {url}"), &link("plain")), "{lines:?}");
    // The junction's name and target both go to the folder it names.
    assert!(
        lines.get(7).is_some_and(|line| line
            .split(" -> ")
            .all(|part| part.starts_with(url) && part.contains("/sub\x1b\\"))),
        "{lines:?}"
    );
    assert_eq!(lines.get(8..), Some(&["plain", "plain"][..]), "{lines:?}");
    assert!(!out.stdout.contains("?/"), "{}", out.stdout);
}

/// `-n`, `-Z` and `--author` as GNU ls 9.10 has them on a system without security
/// contexts: the numbers `stat` gives, `?` for the context, the owner again as the
/// author, beside the owner only.
#[test]
fn ls_numbers_contexts_and_authors_as_gnu_ls_has_them() {
    let scratch = helpers_fixture("ls-poor-fits");
    let out = ls_in(
        scratch.path(),
        "ls -nd --time-style=+T plain | sed 's/ 0 T plain$//; s/^[^ ]* [^ ]* //'; \
         stat -c '%u %g' plain; ls -Z plain; ls -Zmd plain sub; ls -Zs1 plain; \
         ls -l --time-style=+T plain; ls -l --author --time-style=+T plain; \
         ls -lg --author --time-style=+T plain; ls -lg --time-style=+T plain; \
         ls -lZ --time-style=+T plain | sed -n 's/.* \\(? 0 T plain\\)$/\\1/p'; \
         stat -c '%U %G' plain",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(lines.first(), lines.get(1), "{}", out.stdout);
    assert_eq!(
        lines.get(2..5),
        Some(&["? plain", "? plain, ? sub", "0 ? plain"][..]),
        "{}",
        out.stdout
    );
    let (owner, group) = lines.get(10).and_then(|line| line.split_once(' ')).unwrap();
    let long = lines.get(5).copied().unwrap_or_default();
    assert_eq!(
        lines.get(6).copied(),
        Some(&*long.replacen(
            &format!(" {owner} {group} "),
            &format!(" {owner} {group} {owner} "),
            1
        )),
        "{}",
        out.stdout
    );
    assert_eq!(lines.get(7), lines.get(8), "{}", out.stdout);
    assert_eq!(lines.get(9), Some(&"? 0 T plain"), "{}", out.stdout);
}

/// `-D` writes what Emacs' dired reads, as GNU ls 9.10 does: the long format indented by
/// two spaces, then where each name and each folder header's name are, in bytes from
/// the start, and the quoting style; not with `-C` or `--hyperlink`, and never with
/// `--zero`.
#[test]
fn ls_dired_as_gnu_ls_has_it() {
    let scratch = helpers_fixture("ls-dired");
    let out = ls_in(
        scratch.path(),
        "ls -lD --time-style=+T --quoting-style=shell plain 'a b' sub",
    );
    let text = out.stdout.replace("\r\n", "\n");
    let (listing, end) = text.split_once("//DIRED//").unwrap();
    let places = |label: &str| -> Vec<&str> {
        let tail = format!("//DIRED//{end}");
        let numbers: Vec<usize> = tail
            .lines()
            .find_map(|line| line.strip_prefix(label)?.strip_prefix(' '))
            .unwrap_or_default()
            .split_whitespace()
            .map(|number| number.parse().unwrap())
            .collect();
        numbers
            .chunks(2)
            .map(|pair| listing.get(pair[0]..pair[1]).unwrap_or("?"))
            .collect()
    };
    assert_eq!(places("//DIRED//"), ["'a b'", "plain", "inner"], "{text}");
    assert_eq!(places("//SUBDIRED//"), ["sub"], "{text}");
    assert!(
        text.ends_with("//DIRED-OPTIONS// --quoting-style=shell"),
        "{text}"
    );
    let lines: Vec<&str> = listing.lines().collect();
    assert!(
        lines.first().is_some_and(|line| line.starts_with("  -rw")),
        "{text}"
    );
    assert_eq!(lines.get(2..4), Some(&["", "  sub:"][..]), "{text}");
    assert_eq!(lines.get(4), Some(&"  total 0"), "{text}");

    let out = ls_in(
        scratch.path(),
        "ls -D -Cd plain sub; ls -lD --hyperlink=never -1 plain | tail -1; \
         ls -lD --zero plain; echo $?",
    );
    assert_eq!(
        out.stdout, "plain  sub\n//DIRED-OPTIONS// --quoting-style=literal\n2",
        "{}",
        out.stderr
    );
    assert_eq!(out.stderr, "ls: --dired and --zero are incompatible");
}

/// `ls -l` shows a link's target in cash's spelling, `C:/…`, as `readlink` gives it,
/// where it showed Windows' backslashes; and its size is the target's length, as Linux's
/// `lstat` and Git Bash's ls give it, where Windows says 0.
#[test]
fn ls_l_shows_a_links_target_as_readlink_does() {
    let scratch = helpers_fixture("ls-link-target");
    let out = ls_in(
        scratch.path(),
        "ls -l --time-style=+T linkdir | sed -n 's/^.* T //p'; readlink linkdir; \
         ls -l --time-style=+T linkdir | sed 's/ T linkdir.*//; s/.* //'",
    );
    let lines: Vec<&str> = out.stdout.lines().collect();
    let target = lines.get(1).copied().unwrap_or_default();
    assert!(
        target.ends_with("/sub") && !target.contains('\\'),
        "{}",
        out.stdout
    );
    assert_eq!(
        lines.first().copied(),
        Some(&*format!("linkdir -> {target}"))
    );
    assert_eq!(lines.get(2).copied(), Some(&*target.len().to_string()));
}

/// `stat` reads a file's first two bytes to tell whether it runs (`#!`), and NTFS took
/// the read for an access: it moved the access time, which GNU's, reading no file,
/// leaves. It reads through a handle that keeps it now. `ls` reads in bulk and moves it
/// as Git Bash's ls does: keeping it cost 12% of `ls -la` (the user, 2026-10-18); its
/// `x` is still read, which is checked here.
#[test]
fn stat_leaves_a_files_access_time() {
    let scratch = Scratch::new("ls-access-time");
    let accessed = std::time::UNIX_EPOCH + std::time::Duration::from_hours(438_000);
    for (name, text) in [("script", "echo hi\n"), ("shebang", "#!/bin/sh\necho hi\n")] {
        std::fs::write(scratch.join(name), text).unwrap();
        std::fs::File::options()
            .write(true)
            .open(scratch.join(name))
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_accessed(accessed))
            .unwrap();
    }
    let out = ls_in(scratch.path(), "stat -c %A script shebang | cut -c4");
    // The `#!` file runs, the other does not: what the read is for.
    assert_eq!(out.stdout, "-\nx", "{}", out.stderr);
    for name in ["script", "shebang"] {
        let metadata = std::fs::metadata(scratch.join(name)).unwrap();
        assert_eq!(metadata.accessed().unwrap(), accessed, "{name}");
    }
}
