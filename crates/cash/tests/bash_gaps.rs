//! Focused Bash 5.2 compatibility regressions found by the source comparison.
#![cfg(windows)]
#![allow(
    clippy::tests_outside_test_module,
    clippy::unwrap_used,
    clippy::panic,
    clippy::literal_string_with_formatting_args,
    reason = "integration assertions and literal shell snippets"
)]

use std::path::Path;
use std::process::Command;

const CASH: &str = env!("CARGO_BIN_EXE_cash");

fn output(script: &str) -> (i32, String) {
    let result = Command::new(CASH)
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    (
        result.status.code().unwrap_or(-1),
        stdout.trim_end().to_string(),
    )
}

fn output_in(directory: &Path, script: &str) -> (i32, String) {
    let result = Command::new(CASH)
        .current_dir(directory)
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    (
        result.status.code().unwrap_or(-1),
        stdout.trim_end().to_string(),
    )
}

fn source_fixture(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("cash-source-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn bash_53_current_shell_substitution_preserves_side_effects() {
    assert_eq!(
        output("x=old; y=${ x=new; printf 'value\\n\\n'; }; printf '<%s>:<%s>\\n' \"$x\" \"$y\""),
        (0, "<new>:<value>".into())
    );
    assert_eq!(
        output(
            "REPLY=old; x=old; y=${| x=new; REPLY='a\\nb'; printf visible; }; printf '<%s>:<%s>:<%s>\\n' \"$x\" \"$REPLY\" \"$y\""
        ),
        (0, "visible<new>:<old>:<a\\nb>".into())
    );
    assert_eq!(
        output("x=old; y=${ x=new; printf '%s' '; }'; }; printf '<%s>:<%s>\\n' \"$x\" \"$y\""),
        (0, "<new>:<; }>".into())
    );
}

#[test]
fn bash_53_compgen_stores_matches_in_indexed_array() {
    assert_eq!(
        output("compgen -V a -W 'one two'; printf '<%s>\\n' \"${a[@]}\""),
        (0, "<one>\n<two>".into())
    );
    assert_eq!(
        output(
            "a=(old); compgen -V a -W 'one two' zz; status=$?; printf '%s:%s\\n' \"$status\" \"${#a[@]}\""
        ),
        (0, "1:0".into())
    );
}

#[test]
fn source_and_dot_follow_bash_53_path_lookup() {
    let root = source_fixture("path");
    let searched = root.join("searched");
    let work = root.join("work");
    std::fs::create_dir_all(&searched).unwrap();
    std::fs::create_dir_all(&work).unwrap();
    std::fs::write(
        searched.join("probe.sh"),
        "FROM_PATH=yes\nprintf 'path=<%s> args=<%s>\\n' \"$FROM_PATH\" \"$*\"\nreturn 23\n",
    )
    .unwrap();
    std::fs::write(work.join("local.sh"), "LOCAL_SOURCE=yes\n").unwrap();

    let searched = searched.to_string_lossy().replace('\\', "/");
    assert_eq!(
        output_in(
            &work,
            &format!(
                "PATH='{searched}':\"$PATH\"; source probe.sh A B; r=$?; printf 'rc=%s from=%s\\n' \"$r\" \"$FROM_PATH\""
            ),
        ),
        (0, "path=<yes> args=<A B>\nrc=23 from=yes".into())
    );
    assert_eq!(
        output_in(
            &work,
            &format!(
                "PATH=/missing; . -p '{searched}' probe.sh P; r=$?; printf 'rc=%s from=%s\\n' \"$r\" \"$FROM_PATH\""
            ),
        ),
        (0, "path=<yes> args=<P>\nrc=23 from=yes".into())
    );
    assert_eq!(
        output_in(
            &work,
            "shopt -u sourcepath; source local.sh; printf 'local=%s\\n' \"$LOCAL_SOURCE\"",
        ),
        (0, "local=yes".into())
    );
    assert_eq!(
        output_in(
            &work,
            &format!(
                "PATH=:'{searched}'; source local.sh; printf 'empty-entry=%s\\n' \"$LOCAL_SOURCE\""
            ),
        ),
        (0, "empty-entry=yes".into())
    );
    assert_eq!(
        output_in(
            &work,
            "source -p '' local.sh; printf 'empty-override=%s\\n' \"$LOCAL_SOURCE\"",
        ),
        (0, "empty-override=yes".into())
    );
    assert_eq!(
        output_in(
            &work,
            "source -p /missing local.sh; r=$?; printf 'rc=%s local=%s\\n' \"$r\" \"$LOCAL_SOURCE\"",
        ),
        (0, "rc=1 local=".into())
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_argument_mutations_follow_bash_scope_rules() {
    let root = source_fixture("args");
    std::fs::write(
        root.join("mutate.sh"),
        "set -- changed by source\nprintf 'inside=<%s>\\n' \"$*\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("empty.sh"),
        "set --\nprintf 'inside-count=%s\\n' \"$#\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("shift.sh"),
        "shift\nprintf 'shifted=<%s>\\n' \"$*\"\n",
    )
    .unwrap();

    assert_eq!(
        output_in(
            &root,
            "set -- outer; source ./mutate.sh inner; printf 'after=<%s>\\n' \"$*\"",
        ),
        (
            0,
            "inside=<changed by source>\nafter=<changed by source>".into()
        )
    );
    assert_eq!(
        output_in(
            &root,
            "f() { set -- function outer; source ./mutate.sh inner; printf 'function-after=<%s>\\n' \"$*\"; }; f",
        ),
        (
            0,
            "inside=<changed by source>\nfunction-after=<function outer>".into()
        )
    );
    assert_eq!(
        output_in(
            &root,
            "set -- outer; source ./empty.sh inner; printf 'after-count=%s\\n' \"$#\"",
        ),
        (0, "inside-count=0\nafter-count=0".into())
    );
    assert_eq!(
        output_in(
            &root,
            "set -- outer; source ./shift.sh first second; printf 'after-shift=<%s>\\n' \"$*\"",
        ),
        (0, "shifted=<second>\nafter-shift=<outer>".into())
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn source_inherits_debug_trap_only_with_functrace() {
    let root = source_fixture("debug-trap");
    std::fs::write(root.join("body.sh"), "printf 'inside\\n'\n").unwrap();

    assert_eq!(
        output_in(
            &root,
            "trap 'case $BASH_SOURCE in *body.sh) echo inherited-debug;; esac' DEBUG; source ./body.sh; set -T; source ./body.sh",
        ),
        (0, "inside\ninherited-debug\ninside".into())
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn wait_preserves_status_and_waits_for_next_completion() {
    assert_eq!(
        output("bash -c 'exit 7' & wait %1; echo $?"),
        (0, "7".into())
    );
    assert_eq!(
        output("bash -c 'sleep 0.1; exit 7' & wait -n; echo $?"),
        (0, "7".into())
    );
    assert_eq!(
        output("bash -c 'exit 7' & wait -f $!; echo $?"),
        (0, "7".into())
    );
    assert_eq!(output("wait -n; echo $?"), (0, "127".into()));
    assert_eq!(
        output("bash -c 'exit 7' & wait %1; wait -n; echo $?"),
        (0, "127".into())
    );
    let (_, first) = output(
        "bash -c 'sleep 0.2; exit 3' & a=$!; bash -c 'sleep 0.05; exit 7' & b=$!; wait -n $a $b; echo $?; wait $a; echo $?",
    );
    assert_eq!(first, "7\n3");
    let (_, assigned) =
        output("bash -c 'sleep 0.1; exit 7' & wait -n -p done; echo $? ${done:+assigned}");
    assert_eq!(assigned, "7 assigned");
}

#[test]
fn substitution_replacement_uses_shell_semantics() {
    assert_eq!(
        output("x=abc; r='$1'; echo \"${x/b/$r}\""),
        (0, "a$1c".into())
    );
    assert_eq!(output("x=abc; echo \"${x/b/[&]}\""), (0, "a[b]c".into()));
    assert_eq!(output("x=abc; echo \"${x/b/'[&]'}\""), (0, "a[&]c".into()));
    assert_eq!(output("x=abc; echo \"${x/b/\\&}\""), (0, "a&c".into()));
    assert_eq!(
        output("shopt -u patsub_replacement; x=abc; echo \"${x/b/[&]}\""),
        (0, "a[&]c".into())
    );
    assert_eq!(
        output("x=abc; r='[\\&]'; echo \"${x/b/$r}\""),
        (0, "a[&]c".into())
    );
}

#[test]
fn local_dash_restores_set_options() {
    let script = "set +u; f() { local -; set -u; }; f; case $- in *u*) echo leaked;; *) echo restored;; esac";
    assert_eq!(output(script), (0, "restored".into()));
    let nested = "set +u; inner() { local -; set +u; }; outer() { local -; set -u; inner; case $- in *u*) echo nested-restored;; esac; return 4; }; outer; case $- in *u*) echo leaked;; *) echo outer-restored;; esac";
    assert_eq!(
        output(nested),
        (0, "nested-restored\nouter-restored".into())
    );
}

#[test]
fn posix_shell_does_not_load_bash_env() {
    let root = std::env::temp_dir().join(format!("cash-bash-env-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let startup = root.join("startup.sh");
    std::fs::write(&startup, "echo startup\n").unwrap();
    let run = |posix| {
        let mut command = Command::new(CASH);
        command.env("BASH_ENV", startup.to_string_lossy().replace('\\', "/"));
        if posix {
            command.arg("--posix");
        }
        let result = command.args(["-c", "echo body"]).output().unwrap();
        String::from_utf8(result.stdout)
            .unwrap()
            .replace("\r\n", "\n")
            .trim_end()
            .to_string()
    };
    assert_eq!(run(false), "startup\nbody");
    assert_eq!(run(true), "body");
    std::fs::remove_file(startup).unwrap();
    std::fs::remove_dir(root).unwrap();
}

#[test]
fn mapfile_callback_sees_lines_before_assignment() {
    let script = "cb() { printf 'callback:%s:%s:%s\\n' \"$1\" \"$2\" \"${a[0]-unset}\"; }; mapfile -t -C cb -c 1 a <<< hello; printf 'value:%s\\n' \"${a[0]}\"";
    assert_eq!(
        output(script),
        (0, "callback:0:hello:unset\nvalue:hello".into())
    );
    let every_two = "cb() { echo callback:$1:$2; }; mapfile -t -C cb -c 2 -O 3 a <<< $'one\\ntwo\\nthree'; echo ${a[3]}:${a[4]}:${a[5]}";
    assert_eq!(
        output(every_two),
        (0, "callback:4:two\none:two:three".into())
    );
}

#[test]
fn printf_count_assigns_shell_variable() {
    assert_eq!(
        output("printf 'abc%n\\n' n; echo \"$n\""),
        (0, "abc\n3".into())
    );
    assert_eq!(
        output("printf '%s%n' A x B y; echo :$x:$y"),
        (0, "AB:1:2".into())
    );
    assert_eq!(
        output("printf -v out 'abc%n' n; echo :$out:$n"),
        (0, ":abc:3".into())
    );
    assert_eq!(
        output("printf '%5n' n; echo \"${n-unset}\""),
        (0, "0".into())
    );
    assert_eq!(output("printf 'x%n'; echo :$?"), (0, "x:0".into()));
}

#[test]
fn printf_q_precision_matches_bash_ordering() {
    assert_eq!(
        output("printf '<%.2Q> <%.2q> <%5.2Q> <%#.2Q>\\n' 'a b' 'a b' 'a b' 'a b'"),
        (0, "<a\\ > <a\\> <  a\\ > <'a '>".into())
    );
}

#[test]
fn local_print_includes_the_saved_option_scope() {
    assert_eq!(
        output("f() { local x=1; local -; local -p; }; f"),
        (0, "local -\ndeclare -- x=\"1\"".into())
    );
}

#[test]
fn disabling_globskipdots_exposes_dot_and_dotdot() {
    let root = std::env::temp_dir().join(format!("cash-globskipdots-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join(".hidden"), "").unwrap();
    let path = root.to_string_lossy().replace('\\', "/");
    let result = output(&format!(
        "cd '{path}'; shopt -u globskipdots; printf '<%s>\\n' .*"
    ));
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(result, (0, "<.>\n<..>\n<.hidden>".into()));
}

#[test]
fn associative_unset_at_and_star_are_literal_keys() {
    assert_eq!(
        output("declare -A a=([@]=at [*]=star [x]=other); unset 'a[@]'; declare -p a"),
        (0, "declare -A a=([\"*\"]=\"star\" [x]=\"other\" )".into())
    );
    assert_eq!(
        output("declare -A a=([@]=at [*]=star [x]=other); unset 'a[*]'; declare -p a"),
        (0, "declare -A a=([@]=\"at\" [x]=\"other\" )".into())
    );
    assert_eq!(
        output("a=(zero one); unset 'a[@]'; declare -p a"),
        (0, "declare -a a=()".into())
    );
}

#[test]
fn read_e_and_i_accept_redirected_input() {
    assert_eq!(
        output("read -e value <<< hello; printf '<%s>\\n' \"$value\""),
        (0, "<hello>".into())
    );
    assert_eq!(
        output("read -e -i seed value <<< world; printf '<%s>\\n' \"$value\""),
        (0, "<world>".into())
    );
    assert_eq!(
        output("read -E value <<< hello; printf '<%s>\\n' \"$value\""),
        (0, "<hello>".into())
    );
}

#[test]
fn read_matches_source_level_edge_semantics() {
    assert_eq!(
        output("printf abc | { read -n 0 a; read -n 1 b; printf '<%s>:<%s>\\n' \"$a\" \"$b\"; }"),
        (0, "<>:<a>".into())
    );
    assert_eq!(
        output("printf abc | { read -N 0 a; read -N 1 b; printf '<%s>:<%s>\\n' \"$a\" \"$b\"; }"),
        (0, "<>:<a>".into())
    );
    assert_eq!(
        output("printf 'a\\001b\\n' | { IFS= read -r v; printf '%s\\n' \"${#v}\"; }"),
        (0, "3".into())
    );
    assert_eq!(
        output("printf 'a\\\\:b:c' | { IFS= read -d : v; printf '<%s>\\n' \"$v\"; }"),
        (0, "<a:b>".into())
    );
    assert_eq!(
        output("printf abcdef | { read -N 2 -n 3 v; printf '<%s>\\n' \"$v\"; }"),
        (0, "<abc>".into())
    );
    assert_eq!(
        output("printf abcdef | { read -n 4 -N 3 v; printf '<%s>\\n' \"$v\"; }"),
        (0, "<abc>".into())
    );
    assert_eq!(
        output("printf abcdef | { read -n 2 -n 3 v; printf '<%s>\\n' \"$v\"; }"),
        (0, "<abc>".into())
    );
    assert_eq!(
        output("printf abcdef | { read -N 2 -N 3 v; printf '<%s>\\n' \"$v\"; }"),
        (0, "<abc>".into())
    );
}

#[test]
fn read_array_targets_match_bash_assignment_rules() {
    assert_eq!(
        output(
            "a=(zero old); printf new | { read 'a[1]'; printf '%s:%s\\n' \"${a[0]}\" \"${a[1]}\"; }"
        ),
        (0, "zero:new".into())
    );
    assert_eq!(
        output(
            "i=1; a=(zero old); printf new | { read 'a[i]'; printf '%s:%s\\n' \"${a[0]}\" \"${a[1]}\"; }"
        ),
        (0, "zero:new".into())
    );
    assert_eq!(
        output(
            "declare -A a=([key]=old); printf new | { read 'a[key]'; printf '%s\\n' \"${a[key]}\"; }"
        ),
        (0, "new".into())
    );
    assert_eq!(
        output(
            "printf 'one two\\n' | { declare -A a=([key]=old); read -a a; status=$?; IFS= read -r rest; printf '%s:<%s>:<%s>\\n' \"$status\" \"${a[key]}\" \"$rest\"; }"
        ),
        (0, "1:<old>:<>".into())
    );
    assert_eq!(
        output("declare -n ref=result; read ref <<< value; printf '%s:%s\\n' \"$result\" \"$ref\""),
        (0, "value:value".into())
    );
    assert_eq!(
        output(
            "declare -n ref=items; read -a ref <<< 'one two'; printf '%s:%s:%s\\n' \"${items[0]}\" \"${items[1]}\" \"${ref[1]}\""
        ),
        (0, "one:two:two".into())
    );
}

#[test]
fn read_rejects_non_finite_timeouts_and_uses_tmout() {
    assert_eq!(output("read -t nan v </dev/null; echo $?"), (0, "1".into()));
    assert_eq!(output("read -t inf v </dev/null; echo $?"), (0, "1".into()));
    assert_eq!(
        output(
            "TMOUT=.05; { sleep .2; echo late; } | { read v; printf '%s:<%s>\\n' \"$?\" \"$v\"; }"
        ),
        (0, "142:<>".into())
    );
}

#[test]
fn array_k_transform_keeps_key_value_word_boundaries() {
    assert_eq!(
        output("declare -A a=([x]='one two' [y]=z); printf '<%s>\\n' \"${a[@]@k}\""),
        (0, "<x>\n<one two>\n<y>\n<z>".into())
    );
    assert_eq!(
        output("a=('one two' z); printf '<%s>\\n' \"${a[@]@k}\""),
        (0, "<0>\n<one two>\n<1>\n<z>".into())
    );
}

#[test]
fn history_file_operations_expansion_and_fc_editor_mode_work() {
    let root = std::env::temp_dir().join(format!("cash-history-gaps-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let history_file = root.join("history.txt");
    std::fs::write(&history_file, "one\ntwo\n").unwrap();
    let path = history_file.to_string_lossy().replace('\\', "/");

    let script = format!(
        "set -o history; history -c; history -r '{path}'; history -p '!1' '!2'; printf 'three\\n' >> '{path}'; history -n '{path}'; history -p '!3'; history -n '{path}'; history | wc -l"
    );
    assert_eq!(output(&script), (0, "one\ntwo\nthree\n3".into()));

    assert_eq!(
        output(
            "set -o history; history -c; history -s 'printf ORIGINAL'; edit_hist() { printf 'printf EDITED\\n' > \"$1\"; }; FCEDIT=edit_hist; fc 1"
        ),
        (0, "EDITED".into())
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn jobs_n_reports_each_state_change_once() {
    let (_, stdout) =
        output("sleep .2 & jobs -n; echo SECOND; jobs -n; sleep .3; jobs -n; echo FOURTH; jobs -n");
    assert_eq!(stdout.matches("Running").count(), 1, "{stdout}");
    assert_eq!(stdout.matches("Done").count(), 1, "{stdout}");
    assert!(stdout.contains("SECOND\n"), "{stdout}");
    assert!(stdout.contains("FOURTH"), "{stdout}");

    let (_, stdout) = output("true & sleep .1; jobs -n -r; echo FILTERED; jobs -n");
    assert_eq!(stdout.matches("Done").count(), 1, "{stdout}");
    assert!(stdout.starts_with("FILTERED\n"), "{stdout}");
}

#[test]
fn bash_command_and_seconds_are_updated() {
    assert_eq!(
        output("printf 'one=<%s>\\n' \"$BASH_COMMAND\"; printf 'two=<%s>\\n' \"$BASH_COMMAND\""),
        (
            0,
            "one=<printf 'one=<%s>\\n' \"$BASH_COMMAND\">\ntwo=<printf 'two=<%s>\\n' \"$BASH_COMMAND\">"
                .into()
        )
    );
    assert_eq!(
        output("SECONDS=10; printf '%s\\n' \"$SECONDS\"; SECONDS=-2; printf '%s\\n' \"$SECONDS\""),
        (0, "10\n-2".into())
    );
}

#[test]
fn symbolic_umask_updates_the_remembered_mask() {
    assert_eq!(
        output(
            "umask 022; umask u=rwx,g=rx,o=; umask; umask -S; umask u+w,g-r,o+x; umask; umask -S"
        ),
        (0, "0027\nu=rwx,g=rx,o=\n0066\nu=rwx,g=x,o=x".into())
    );
    assert_eq!(
        output("umask 0677; umask g=ru; umask -S; umask 0677; umask g=ur; umask -S"),
        (0, "u=x,g=x,o=\nu=x,g=rx,o=".into())
    );
}

#[test]
fn enable_loadable_builtin_options_report_platform_unavailability() {
    assert_eq!(
        output(
            "enable -d echo; printf 'd=%s\\n' \"$?\"; enable -f /missing echo; printf 'f=%s\\n' \"$?\""
        ),
        (0, "d=2\nf=2".into())
    );
}
