//! Generated completion scripts — **D40**.
//!
//! Every serious CLI ships a `<tool> completion bash` subcommand, and people expect to
//! source the result. Two families exist and they fail differently:
//!
//! - **clap-generated** (rustup, cargo, ripgrep, most Rust CLIs) are self-contained pure
//!   bash and work in any bash-compatible shell.
//! - **Cobra-generated** (docker, kubectl, gh, helm, argocd — most of the Go ecosystem)
//!   call `_get_comp_words_by_ref` and `_filedir`, which live in the `bash-completion`
//!   *package* rather than in bash itself.
//!
//! On Linux a distro installs that package, so nobody notices. On Windows there is
//! nothing to install — Git for Windows does not ship it either — so sourcing
//! `docker completion bash` succeeds and then silently completes nothing at all. cash
//! defines the helpers itself, which is the whole fix.
//!
//! The synthetic script below is shaped exactly like a Cobra one, so these tests hold
//! whether or not docker happens to be installed on the machine running them. The tests
//! against real tools run opportunistically on top.

#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    reason = "an integration test is outside a test module by construction, and a \
              failed assumption in a test should abort it loudly rather than be \
              threaded back through a Result. Shell snippets are spelled with hashes \
              throughout, including where they are not strictly needed, because \
              alternating the two forms by accident of content reads worse."
)]

use std::path::{Path, PathBuf};
use std::process::Command;

use cash_builtins::ShellBuilderExt as _;
use cash_core::Shell;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

/// A miniature of a Cobra-generated script: the same helper calls, the same fallback
/// probe, the same `COMPREPLY` protocol — without needing docker installed.
const COBRA_SHAPED: &str = r#"
__mytool_init_completion() {
    COMPREPLY=()
    _get_comp_words_by_ref "$@" cur prev words cword
}

__start_mytool() {
    local cur prev words cword

    COMPREPLY=()

    if declare -F _init_completion >/dev/null 2>&1; then
        _init_completion -n =: || return
    else
        __mytool_init_completion -n =: || return
    fi

    local candidates
    case "${words[1]}" in
        run)    candidates="run-a run-b" ;;
        *)      candidates="run build push" ;;
    esac

    local c
    for c in $candidates; do
        case "$c" in
            "$cur"*) COMPREPLY+=("$c") ;;
        esac
    done
}

complete -o default -F __start_mytool mytool
"#;

struct Fixture {
    shell: Shell,
    dir: PathBuf,
}

impl Fixture {
    async fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("cash-compscript-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture dir");

        let mut shell = Shell::builder()
            .interactive(true)
            .profile(cash_core::ProfileLoadBehavior::Skip)
            .rc(cash_core::RcLoadBehavior::Skip)
            .default_builtins(cash_builtins::BuiltinSet::BashMode)
            .build()
            .await
            .expect("build shell");

        shell.set_working_dir(&dir).expect("set working dir");

        Self { shell, dir }
    }

    async fn source(&mut self, script: &str) {
        let path = self.dir.join("completion.bash");
        std::fs::write(&path, script.as_bytes()).expect("write script");
        self.source_file(&path).await;
    }

    async fn source_file(&mut self, path: &Path) {
        let params = self.shell.default_exec_params();
        let result = self
            .shell
            .source_script(path, std::iter::empty::<String>(), &params)
            .await
            .expect("sourcing the completion script failed");
        assert!(result.is_success(), "the completion script exited non-zero");
    }

    async fn complete(&mut self, input: &str) -> Vec<String> {
        self.shell
            .complete(input, input.len())
            .await
            .expect("completion failed")
            .candidates
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

/// Run a snippet in a real interactive cash process (where the shims are defined) and
/// return trimmed stdout.
fn cash(script: &str) -> String {
    let out = Command::new(CASH)
        .args(["--norc", "-i", "-c", script])
        .env("HISTFILE", "")
        .output()
        .expect("failed to run cash");
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

// ---------------------------------------------------------------------------
// The shims exist and behave
// ---------------------------------------------------------------------------

#[test]
fn a_script_does_not_see_the_helpers() {
    // Non-interactive shells get Bash's function table: no completion shims.
    let out = Command::new(CASH)
        .args([
            "--norc",
            "-c",
            "declare -F _get_comp_words_by_ref _filedir _init_completion; echo rc=$?",
        ])
        .output()
        .expect("failed to run cash");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(stdout.trim_end(), "rc=1", "{stdout}");
}

#[test]
fn the_helpers_are_defined_without_any_rc_file() {
    // They are loaded before rc files precisely so that a bare `cash` has them.
    let defined = cash("declare -F _get_comp_words_by_ref _filedir _init_completion");
    for name in ["_get_comp_words_by_ref", "_filedir", "_init_completion"] {
        assert!(defined.contains(name), "{name} is not defined: {defined}");
    }
}

#[test]
fn get_comp_words_by_ref_splits_a_line_at_the_cursor() {
    let probe = r#"
        f() {
            local cur prev words cword
            _get_comp_words_by_ref -n =: cur prev words cword
            printf '%s|%s|%s|%s\n' "$cur" "$prev" "$cword" "${#words[@]}"
        }
        COMP_LINE='docker ru'; COMP_POINT=9; f
    "#;
    assert_eq!(cash(probe), "ru|docker|1|2");
}

#[test]
fn a_trailing_space_starts_a_new_empty_word() {
    // The difference between "complete the word I am typing" and "suggest the next
    // argument". Cobra's scripts branch on exactly this.
    let probe = r#"
        f() {
            local cur prev words cword
            _get_comp_words_by_ref -n =: cur prev words cword
            printf '%s|%s|%s|%s\n' "$cur" "$prev" "$cword" "${#words[@]}"
        }
        COMP_LINE='docker run '; COMP_POINT=11; f
    "#;
    assert_eq!(cash(probe), "|run|2|3");
}

#[test]
fn an_empty_line_yields_one_empty_word() {
    let probe = r#"
        f() {
            local cur prev words cword
            _get_comp_words_by_ref -n =: cur prev words cword
            printf '%s|%s|%s|%s\n' "$cur" "$prev" "$cword" "${#words[@]}"
        }
        COMP_LINE=''; COMP_POINT=0; f
    "#;
    // cword is 0 and there is one word, which is empty: the cursor is at the start of a
    // command line nobody has typed anything into yet.
    assert_eq!(cash(probe), "||0|1");
}

#[test]
fn the_exclude_list_keeps_an_equals_inside_one_word() {
    // What `-n =:` is asking for. Splitting `--namespace=kube-system` in two would send
    // the tool a request it cannot parse.
    let probe = r#"
        f() {
            local cur prev words cword
            _get_comp_words_by_ref -n =: cur prev words cword
            printf '%s|%s|%s\n' "$cur" "${words[1]}" "${#words[@]}"
        }
        COMP_LINE='kubectl --namespace=kube-system get po'; COMP_POINT=38; f
    "#;
    assert_eq!(cash(probe), "po|--namespace=kube-system|4");
}

#[test]
fn the_cursor_position_bounds_the_current_word() {
    // The cursor is not always at the end of the line: someone may have moved back.
    let probe = r#"
        f() {
            local cur prev words cword
            _get_comp_words_by_ref -n =: cur prev words cword
            printf '%s|%s\n' "$cur" "$cword"
        }
        COMP_LINE='docker running'; COMP_POINT=9; f
    "#;
    assert_eq!(
        cash(probe),
        "ru|1",
        "text after the cursor leaked into the word"
    );
}

#[test]
fn filedir_adds_candidates_for_the_current_word() {
    let probe = r#"
        d=$(mktemp -d)
        cd "$d"
        : > alpha.txt
        : > beta.txt
        mkdir gamma
        f() {
            local cur=al
            COMPREPLY=()
            _filedir
            printf '%s\n' "${COMPREPLY[@]}"
        }
        f
        cd /; rm -rf "$d"
    "#;
    assert_eq!(cash(probe), "alpha.txt");
}

#[test]
fn filedir_d_offers_only_directories() {
    let probe = r#"
        d=$(mktemp -d)
        cd "$d"
        : > thing.txt
        mkdir thingdir
        f() {
            local cur=thing
            COMPREPLY=()
            _filedir -d
            printf '%s\n' "${COMPREPLY[@]}"
        }
        f
        cd /; rm -rf "$d"
    "#;
    assert_eq!(cash(probe), "thingdir");
}

#[test]
fn an_rc_file_can_override_a_shim() {
    // The shims load before rc files so that a user who installs the real
    // bash-completion package gets theirs. Proven by overriding one.
    let dir = std::env::temp_dir().join("cash-compscript-override");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create dir");
    let rc = dir.join("rc.sh");
    std::fs::write(&rc, b"_filedir() { echo overridden; }\n").expect("write rc");

    let out = Command::new(CASH)
        .args([
            "--rcfile",
            &rc.to_string_lossy().replace('\\', "/"),
            "-i",
            "-c",
            "_filedir",
        ])
        .env("HISTFILE", "")
        .output()
        .expect("failed to run cash");

    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("overridden"),
        "an rc file could not override a shim: {stdout} {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------
// A Cobra-shaped script, end to end
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_cobra_shaped_script_completes_a_subcommand() {
    let mut fixture = Fixture::new("cobra").await;
    fixture.source(COBRA_SHAPED).await;

    let candidates = fixture.complete("mytool ru").await;
    assert_eq!(candidates, vec!["run".to_string()]);
}

#[tokio::test]
async fn a_cobra_shaped_script_sees_the_preceding_word() {
    // `words[1]` is what the script branches on, so this proves the array — not just
    // `cur` — is reaching it intact.
    let mut fixture = Fixture::new("cobra-words").await;
    fixture.source(COBRA_SHAPED).await;

    let candidates = fixture.complete("mytool run run-").await;
    assert_eq!(
        candidates,
        vec!["run-a".to_string(), "run-b".to_string()],
        "the script did not see `run` as words[1]"
    );
}

#[tokio::test]
async fn a_cobra_shaped_script_offers_everything_for_an_empty_word() {
    let mut fixture = Fixture::new("cobra-empty").await;
    fixture.source(COBRA_SHAPED).await;

    // Sorted, because the spec did not ask for `-o nosort`. That is bash's rule, and
    // the shims must not quietly change it.
    let candidates = fixture.complete("mytool ").await;
    assert_eq!(
        candidates,
        vec!["build".to_string(), "push".to_string(), "run".to_string()]
    );
}

// ---------------------------------------------------------------------------
// Real generated scripts, where the machine has the tool
// ---------------------------------------------------------------------------

/// Generate a tool's bash completion script, or `None` if the tool is not installed.
fn generate(tool: &str, args: &[&str], into: &Path) -> Option<PathBuf> {
    let output = Command::new(tool).args(args).output().ok()?;
    if !output.status.success() || output.stdout.is_empty() {
        return None;
    }

    let path = into.join(format!("{tool}.bash"));
    std::fs::write(&path, &output.stdout).ok()?;
    Some(path)
}

async fn assert_real_tool_completes(tool: &str, args: &[&str], input: &str, expected: &str) {
    let mut fixture = Fixture::new(tool).await;
    let dir = fixture.dir.clone();

    let Some(script) = generate(tool, args, &dir) else {
        eprintln!("skipped: {tool} is not installed");
        return;
    };

    fixture.source_file(&script).await;

    let candidates = fixture.complete(input).await;
    assert!(
        candidates.iter().any(|c| c.trim_end() == expected),
        "{tool}: completing {input:?} did not offer {expected:?}: {candidates:?}"
    );
}

#[tokio::test]
async fn docker_completion_works() {
    // Cobra. Before the shims this sourced cleanly and then completed nothing.
    assert_real_tool_completes("docker", &["completion", "bash"], "docker ru", "run").await;
}

#[tokio::test]
async fn kubectl_completion_works() {
    assert_real_tool_completes(
        "kubectl",
        &["completion", "bash"],
        "kubectl api-r",
        "api-resources",
    )
    .await;
}

#[tokio::test]
async fn gh_completion_works() {
    assert_real_tool_completes("gh", &["completion", "-s", "bash"], "gh pr cr", "create").await;
}

#[tokio::test]
async fn a_clap_generated_script_works_too() {
    // These never needed the shims; asserting it keeps the shims from breaking them.
    assert_real_tool_completes(
        "rustup",
        &["completions", "bash"],
        "rustup tool",
        "toolchain",
    )
    .await;
}
