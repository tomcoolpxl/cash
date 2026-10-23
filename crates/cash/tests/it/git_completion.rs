//! Git's own tab completion, loaded the first time a word after `git` is completed when
//! nothing else completes git, as Git Bash completes it with no setup.
//!
//! A fake Git for Windows root stands in for the real one in most tests: its
//! `git-completion.bash` records each load and tries the things that must not leak into
//! the user's shell. The last test runs the real script, from the Git on `PATH`.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly"
)]

use cash_builtins::ShellBuilderExt as _;
use cash_core::Shell;

use crate::common::Scratch;

/// The fake `git-completion.bash`: completes `git` from a word list, and on the way
/// prints, fails, reads an unset variable, changes options and defines a function that
/// uses a name the user aliased.
const FAKE_SCRIPT: &str = r#"
loads=$((${loads:-0} + 1))
echo "noise on stdout"
echo "noise on stderr" >&2
echo "$never_set" > /dev/null
false
shopt -s nullglob
set -f
__fake_uses_alias() { greet; }
__git_main() { COMPREPLY=($(compgen -W "status stash switch" -- "$2")); }
complete -F __git_main git
complete -F __git_main gitk
false
"#;

struct Fixture {
    shell: Shell,
    dir: Scratch,
}

impl Fixture {
    /// An interactive shell whose `PATH` holds only a fake Git for Windows' `cmd`.
    async fn new(name: &str) -> Self {
        let dir = Scratch::new(&format!("gitcomp-{name}"));
        let completion = dir.join("Git/mingw64/share/git/completion");
        std::fs::create_dir_all(&completion).unwrap();
        std::fs::write(completion.join("git-completion.bash"), FAKE_SCRIPT).unwrap();
        std::fs::create_dir_all(dir.join("Git/cmd")).unwrap();
        std::fs::write(dir.join("Git/cmd/git.exe"), "").unwrap();

        let mut shell = Shell::builder()
            .interactive(true)
            .profile(cash_core::ProfileLoadBehavior::Skip)
            .rc(cash_core::RcLoadBehavior::Skip)
            .default_builtins(cash_builtins::BuiltinSet::BashMode)
            .build()
            .await
            .expect("build shell");
        shell.set_working_dir(dir.path()).expect("set working dir");
        let mut fixture = Self { shell, dir };
        let cmd = fixture.dir.join("Git/cmd");
        fixture.run(&format!("PATH='{}'", cmd.display())).await;
        fixture
    }

    async fn run(&mut self, script: &str) {
        let params = self.shell.default_exec_params();
        let source = cash_core::SourceInfo::from("test");
        let _ = self.shell.run_string(script, &source, &params).await;
    }

    /// What `script` writes to standard output, by way of a file.
    async fn output(&mut self, script: &str) -> String {
        let out = self.dir.join("out.txt");
        self.run(&format!("{{ {script}; }} > '{}' 2>&1", out.display()))
            .await;
        std::fs::read_to_string(out).unwrap().trim_end().to_owned()
    }

    async fn complete(&mut self, input: &str) -> Vec<String> {
        self.shell
            .complete(input, input.len())
            .await
            .expect("completion failed")
            .candidates
    }
}

#[tokio::test]
async fn the_first_git_completion_loads_gits_script_once() {
    let mut fixture = Fixture::new("once").await;
    assert_eq!(fixture.complete("git st").await, ["stash", "status"]);
    assert_eq!(fixture.complete("git sw").await, ["switch"]);
    assert_eq!(fixture.complete("gitk st").await, ["stash", "status"]);
    assert_eq!(fixture.output("echo $loads").await, "1");
}

#[tokio::test]
async fn completing_the_command_word_does_not_load_it() {
    let mut fixture = Fixture::new("word").await;
    fixture.complete("gi").await;
    assert_eq!(fixture.output("echo ${loads-unset}").await, "unset");
}

#[tokio::test]
async fn loading_leaves_the_status_and_options_alone() {
    let mut fixture = Fixture::new("status").await;
    fixture.run("(exit 3)").await;
    assert_eq!(fixture.complete("git st").await, ["stash", "status"]);
    assert_eq!(fixture.output("echo $?").await, "3");
    assert_eq!(
        fixture
            .output("shopt -p nullglob; [[ $- == *f* ]] || echo globbing")
            .await,
        "shopt -u nullglob\nglobbing"
    );
}

#[tokio::test]
async fn any_completion_function_leaves_the_status_alone() {
    let mut fixture = Fixture::new("function").await;
    fixture
        .run("_mine() { COMPREPLY=(one); false; }; complete -F _mine mine")
        .await;
    fixture.run("(exit 3)").await;
    assert_eq!(fixture.complete("mine o").await, ["one"]);
    assert_eq!(fixture.output("echo $?").await, "3");
}

#[tokio::test]
async fn set_e_and_set_u_do_not_cut_the_load_short() {
    // The script fails and reads an unset variable before it defines its completion.
    let mut fixture = Fixture::new("eu").await;
    fixture.run("set -eu").await;
    assert_eq!(fixture.complete("git st").await, ["stash", "status"]);
    let flags = fixture.output("echo $-").await;
    assert!(flags.contains('e') && flags.contains('u'), "{flags}");
}

#[tokio::test]
async fn loading_runs_no_trap_and_expands_no_alias() {
    let mut fixture = Fixture::new("quiet").await;
    fixture
        .run("alias greet='echo expanded'; debugs=0; trap 'debugs=$((debugs+1))' DEBUG")
        .await;
    let before: u32 = fixture.output("echo $debugs").await.parse().unwrap();
    assert_eq!(fixture.complete("git st").await, ["stash", "status"]);
    let after: u32 = fixture.output("echo $debugs").await.parse().unwrap();
    // One DEBUG for the `echo` that shows the count, none for the script's commands.
    assert_eq!(after, before + 1);
    fixture.run("trap - DEBUG").await;
    assert!(
        fixture
            .output("declare -f __fake_uses_alias")
            .await
            .contains("greet"),
        "the user's alias was expanded into the script's function"
    );
}

#[tokio::test]
async fn the_scripts_output_does_not_reach_the_terminal() {
    // The fixture's standard output and error are the test's own; a load that printed
    // would show there, so this checks the load leaves no file of output either way.
    let mut fixture = Fixture::new("output").await;
    let out = fixture.dir.join("load.txt");
    fixture
        .run(&format!("exec 3>&1 4>&2 1>'{0}' 2>&1", out.display()))
        .await;
    fixture.complete("git st").await;
    fixture.run("exec 1>&3 2>&4").await;
    assert_eq!(std::fs::read_to_string(out).unwrap(), "");
}

#[tokio::test]
async fn a_completion_the_user_set_up_is_kept() {
    for setup in [
        "complete -W mine git",
        "_git() { :; }",
        "__git_main() { :; }",
        "complete -D -W mine",
    ] {
        let mut fixture = Fixture::new("own").await;
        fixture.run(setup).await;
        fixture.complete("git st").await;
        assert_eq!(
            fixture.output("echo ${loads-unset}").await,
            "unset",
            "{setup}"
        );
    }
}

#[tokio::test]
async fn without_git_for_windows_nothing_is_loaded() {
    let mut fixture = Fixture::new("none").await;
    fixture.run("PATH=''").await;
    fixture.complete("git st").await;
    assert_eq!(fixture.output("echo ${loads-unset}").await, "unset");
}

#[tokio::test]
async fn the_real_script_completes_git_subcommands() {
    // The Git for Windows on `PATH`, which the tests need (`git_for_windows`).
    let _ = crate::common::git_for_windows();
    let mut shell = Shell::builder()
        .interactive(true)
        .profile(cash_core::ProfileLoadBehavior::Skip)
        .rc(cash_core::RcLoadBehavior::Skip)
        .default_builtins(cash_builtins::BuiltinSet::BashMode)
        .build()
        .await
        .expect("build shell");
    let candidates = shell
        .complete("git checko", "git checko".len())
        .await
        .expect("completion failed")
        .candidates;
    assert!(
        candidates.iter().any(|c| c.trim_end() == "checkout"),
        "{candidates:?}"
    );
}
