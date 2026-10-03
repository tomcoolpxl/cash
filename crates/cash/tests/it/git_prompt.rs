//! Git's own prompt script, `git-prompt.sh` (`__git_ps1`), as Git for Windows installs it.
//!
//! It is the prompt most bash users on Windows already have, and it is dense bash: the
//! tab-separated output of `git rev-list --count --left-right` taken apart with
//! `${count#0<TAB>}`, `IFS=$'\r\n' read`, `printf -v`, a `<<-` here-document, and a
//! `PROMPT_COMMAND` mode that puts `${__git_ps1_branch_name}` in `PS1` rather than the branch
//! itself. The tests source the installed script in a scratch repository, and skip when Git
//! for Windows is not installed.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    reason = "an integration test is outside a test module by construction"
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::common::{CASH, ISOLATED_VARIABLES};

/// A scratch directory, removed on drop, whose git sees neither the user's nor the system's
/// configuration and makes the same commits every run.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("cash-git-prompt-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self { dir }
    }

    fn command(&self, program: &str, cwd: &Path) -> Command {
        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .env("GIT_CONFIG_GLOBAL", self.dir.join("gitconfig"))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "T")
            .env("GIT_AUTHOR_EMAIL", "t@example.com")
            .env("GIT_AUTHOR_DATE", "2026-01-01T00:00:00Z")
            .env("GIT_COMMITTER_NAME", "T")
            .env("GIT_COMMITTER_EMAIL", "t@example.com")
            .env("GIT_COMMITTER_DATE", "2026-01-01T00:00:00Z")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE");
        command
    }

    fn git(&self, cwd: &Path, args: &[&str]) {
        let output = self.command("git", cwd).args(args).output().unwrap();
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    /// Run `script` in cash from `cwd` and return its stdout, CRLF normalized.
    fn cash(&self, cwd: &Path, script: &str, env: &[(&str, &Path)]) -> String {
        // `cash_command()`'s isolation, on a command that already has git's.
        let mut command = self.command(CASH, cwd);
        command.args(["--no-config", "--noprofile", "--norc", "-c", script]);
        for name in ISOLATED_VARIABLES {
            command.env_remove(name);
        }
        for (name, value) in env {
            command.env(name, value);
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "cash exited {:?}: {}",
            output.status.code(),
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout)
            .unwrap()
            .replace("\r\n", "\n")
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// The installed `git-prompt.sh`, found the way Git for Windows' own
/// `/etc/profile.d/git-prompt.sh` finds it; `None` without Git for Windows.
fn installed_git_prompt() -> Option<PathBuf> {
    let output = Command::new("git").arg("--exec-path").output().ok()?;
    let exec_path = String::from_utf8(output.stdout).ok()?;
    let prefix = exec_path.trim_end().strip_suffix("/libexec/git-core")?;
    let script = Path::new(prefix).join("share/git/completion/git-prompt.sh");
    script.is_file().then_some(script)
}

#[test]
fn a_tab_inside_a_parameter_expansion_stays_a_tab() {
    // `git rev-list --count --left-right` separates its two counts with a tab, and
    // git-prompt.sh takes them apart with patterns that hold a literal one. cash's tokenizer
    // put a space back in the tab's place inside `${ }`, so none of them matched.
    let scratch = Scratch::new("tab");
    let script = "count=$'2\\t1'; unset u; \
                  printf '<%s>' \"${count#2\t}\" \"${count%\t1}\" \"${count#*\t}\" \
                  ${count/\t/+} \"${u:-a\tb}\"";
    assert_eq!(
        scratch.cash(&scratch.dir, script, &[]),
        "<1><2><1><2+1><a\tb>"
    );
}

#[test]
fn git_ps1_shows_the_branch_its_state_and_the_upstream() {
    let Some(git_prompt) = installed_git_prompt() else {
        eprintln!("skipped: Git for Windows' git-prompt.sh is not installed");
        return;
    };
    let scratch = Scratch::new("ps1");
    let work = scratch.dir.join("work");
    let origin = scratch.dir.join("origin.git");

    // `work` is two commits ahead of origin/main and one behind it.
    scratch.git(&scratch.dir, &["init", "-q", "-b", "main", "work"]);
    std::fs::write(work.join("a.txt"), "one\n").unwrap();
    scratch.git(&work, &["add", "a.txt"]);
    scratch.git(&work, &["commit", "-qm", "c1"]);
    scratch.git(&work, &["tag", "v1"]);
    std::fs::write(work.join("a.txt"), "one\ntwo\n").unwrap();
    scratch.git(&work, &["commit", "-qam", "c2"]);
    scratch.git(
        &scratch.dir,
        &["clone", "-q", "--bare", "work", "origin.git"],
    );
    scratch.git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    scratch.git(&work, &["fetch", "-q", "origin"]);
    scratch.git(&work, &["branch", "-q", "-u", "origin/main"]);
    scratch.git(&work, &["reset", "-q", "--hard", "HEAD~1"]);
    for name in ["b.txt", "c.txt"] {
        std::fs::write(work.join(name), "x\n").unwrap();
        scratch.git(&work, &["add", name]);
        scratch.git(&work, &["commit", "-qm", name]);
    }

    let script = r#"
        . "$GIT_PROMPT_SH" || exit 99
        show() { printf '%s=[%s]\n' "$1" "$2"; }
        show plain "$(__git_ps1)"
        show format "$(__git_ps1 '<%s>')"
        show auto "$(GIT_PS1_SHOWUPSTREAM=auto __git_ps1)"
        show verbose "$(GIT_PS1_SHOWUPSTREAM='verbose name' __git_ps1)"
        show legacy "$(GIT_PS1_SHOWUPSTREAM='legacy verbose' __git_ps1)"
        (exit 7); __git_ps1 '\W' ' > '; show status "$?"
        show pcmode "$PS1"
        show expanded "${PS1@P}"
        git checkout -q v1; show detached "$(__git_ps1)"; git checkout -q main
        echo stashed >> a.txt; git stash -q
        echo staged >> a.txt; git add a.txt; echo unstaged >> a.txt; : > new.txt
        GIT_PS1_SHOWDIRTYSTATE=1 GIT_PS1_SHOWSTASHSTATE=1 GIT_PS1_SHOWUNTRACKEDFILES=1
        show flags "$(__git_ps1)"
        cd .git && show gitdir "$(__git_ps1)"
    "#;
    assert_eq!(
        scratch.cash(&work, script, &[("GIT_PROMPT_SH", &git_prompt)]),
        [
            "plain=[ (main)]",
            "format=[<main>]",
            "auto=[ (main <>)]",
            "verbose=[ (main|u+2-1 origin/main)]",
            "legacy=[ (main|u+2-1)]",
            "status=[7]",
            r"pcmode=[\W (${__git_ps1_branch_name}) > ]",
            "expanded=[work (main) > ]",
            "detached=[ ((v1))]",
            "flags=[ (main *+$%)]",
            "gitdir=[ (GIT_DIR!)]",
            "",
        ]
        .join("\n")
    );
}
