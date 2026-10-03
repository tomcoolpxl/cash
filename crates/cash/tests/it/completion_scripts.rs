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

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

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
        let dir =
            std::env::temp_dir().join(format!("cash-compscript-{name}-{}", std::process::id()));
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
    let dir = std::env::temp_dir().join(format!("cash-compscript-override-{}", std::process::id()));
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

/// How long a tool gets to answer on its own, before cash is involved at all.
///
/// Warm, each of these tools answers in under a second. The first launch of docker,
/// kubectl and gh on a fresh GitHub runner took 10 to 37 s: measured under `cargo test`,
/// when these were the only tests running, and a nextest retry of the same test then
/// passed in under a second. That is the tool's cold start, not cash, so it is paid here,
/// outside cash, and a tool that has still not answered is skipped rather than failed.
/// `.config/nextest.toml` gives these tests the time this takes, plus cash's share.
const TOOL_DEADLINE: Duration = Duration::from_secs(60);

/// How a generated script finds its candidates (see the module docs).
#[derive(Clone, Copy)]
enum Family {
    /// Pure bash: the candidates are in the script.
    Clap,
    /// The script asks the tool for them, as `tool __complete words…` inside `$(…)`.
    Cobra,
}

/// Run one of a tool's own commands, outside cash, and return what it printed.
///
/// `Err` says why the tool cannot be tested here: it is not installed, it failed, or it
/// was still running at `deadline`, in which case it is killed.
fn run_tool(tool: &str, args: &[&str], deadline: Instant) -> Result<Vec<u8>, String> {
    let command = format!("`{tool} {}`", args.join(" "));
    let started = Instant::now();
    let mut child = Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => format!("{tool} is not installed"),
            _ => format!("{command} could not start: {e}"),
        })?;

    // Read on a thread of its own, so that the deadline holds even while the tool is
    // silent.
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (sender, receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        let _ = sender.send(bytes);
    });

    let output = receiver.recv_timeout(deadline.saturating_duration_since(Instant::now()));
    let seconds = started.elapsed().as_secs_f64();
    let Ok(output) = output else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!("{command} was still running after {seconds:.1} s"));
    };

    let status = child.wait().map_err(|e| format!("{command}: {e}"))?;
    if !status.success() || output.is_empty() {
        return Err(format!("{command} failed ({status})"));
    }
    eprintln!("{command} answered in {seconds:.1} s");
    Ok(output)
}

/// The request a Cobra script sends its tool for `input`: `__complete` and the words up
/// to the cursor, the last one empty after a trailing space.
fn cobra_request(input: &str) -> Vec<&str> {
    let mut request = vec!["__complete"];
    request.extend(input.split_whitespace().skip(1));
    if input.ends_with(' ') {
        request.push("");
    }
    request
}

async fn assert_real_tool_completes(
    family: Family,
    tool: &str,
    args: &[&str],
    input: &str,
    expected: &str,
) {
    let mut fixture = Fixture::new(tool).await;

    // Everything the tool does on its own shares one deadline.
    let deadline = Instant::now() + TOOL_DEADLINE;
    let script = match run_tool(tool, args, deadline) {
        Ok(script) => String::from_utf8(script).expect("the generated script is UTF-8"),
        Err(why) => {
            eprintln!("skipped: {why}");
            return;
        }
    };

    // A Cobra script runs the tool again for every completion, and on a cold machine that
    // second launch is slow too: docker, for one, starts each of its CLI plugins to ask
    // for its commands. Making the same request once here warms all of it up, and says
    // what the tool itself offers.
    let answer = match family {
        Family::Clap => None,
        Family::Cobra => match run_tool(tool, &cobra_request(input), deadline) {
            Ok(answer) => Some(String::from_utf8_lossy(&answer).into_owned()),
            Err(why) => {
                eprintln!("skipped: {why}");
                return;
            }
        },
    };

    // From here on the time is cash's.
    let started = Instant::now();
    fixture.source(&script).await;
    let candidates = fixture.complete(input).await;
    eprintln!(
        "cash completed {input:?} in {:.1} s",
        started.elapsed().as_secs_f64()
    );

    assert!(
        candidates.iter().any(|c| c.trim_end() == expected),
        "{tool}: completing {input:?} did not offer {expected:?}: {candidates:?}\n\
         {tool} itself answered: {answer:?}"
    );
}

#[test]
fn a_tool_that_misses_the_deadline_is_stopped_and_reported() {
    // The deadline is what turns a tool's cold start into a skip rather than a nextest
    // timeout, so it has to hold against a program that is nowhere near done.
    let started = Instant::now();
    let deadline = started + Duration::from_secs(1);
    let why = run_tool(CASH, &["--norc", "-c", "sleep 30"], deadline)
        .expect_err("a 30 s sleep finished before a 1 s deadline");

    assert!(why.contains("was still running"), "{why}");
    assert!(
        started.elapsed() < Duration::from_secs(10),
        "the deadline did not hold: {:?}",
        started.elapsed()
    );
}

#[test]
fn a_tool_that_is_not_installed_is_reported_as_such() {
    let deadline = Instant::now() + Duration::from_secs(10);
    let why = run_tool("cash-test-no-such-tool", &["completion", "bash"], deadline)
        .expect_err("a missing tool ran");
    assert_eq!(why, "cash-test-no-such-tool is not installed");
}

#[test]
fn a_cobra_request_is_the_words_up_to_the_cursor() {
    assert_eq!(cobra_request("docker ru"), ["__complete", "ru"]);
    assert_eq!(cobra_request("gh pr cr"), ["__complete", "pr", "cr"]);
    assert_eq!(cobra_request("docker run "), ["__complete", "run", ""]);
}

#[tokio::test]
async fn docker_completion_works() {
    // Cobra. Before the shims this sourced cleanly and then completed nothing.
    assert_real_tool_completes(
        Family::Cobra,
        "docker",
        &["completion", "bash"],
        "docker ru",
        "run",
    )
    .await;
}

#[tokio::test]
async fn kubectl_completion_works() {
    assert_real_tool_completes(
        Family::Cobra,
        "kubectl",
        &["completion", "bash"],
        "kubectl api-r",
        "api-resources",
    )
    .await;
}

#[tokio::test]
async fn gh_completion_works() {
    assert_real_tool_completes(
        Family::Cobra,
        "gh",
        &["completion", "-s", "bash"],
        "gh pr cr",
        "create",
    )
    .await;
}

#[tokio::test]
async fn a_clap_generated_script_works_too() {
    // These never needed the shims; asserting it keeps the shims from breaking them.
    assert_real_tool_completes(
        Family::Clap,
        "rustup",
        &["completions", "bash"],
        "rustup tool",
        "toolchain",
    )
    .await;
}
