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
fn bash_52_heredoc_understands_dollar_quoted_patterns() {
    let script = "v=$'x\\tY'; cat <<EOF\n<${v#$'x\\t'}>\n<${v#x$\"\t\"}>\nEOF";
    assert_eq!(output(script), (0, "<Y>\n<Y>".into()));

    let quoted = "v=$'x\\tY'; cat <<'EOF'\n<${v#$'x\\t'}>\nEOF";
    assert_eq!(output(quoted), (0, "<${v#$'x\\t'}>".into()));
}

#[test]
fn bash_52_command_p_ignores_a_poisoned_hash_entry() {
    let (_, normal) = output("hash -p C:/definitely/missing/where.exe where; command -v where");
    assert!(normal.contains("definitely/missing"), "{normal}");

    let (status, standard) =
        output("hash -p C:/definitely/missing/where.exe where; command -p -v where");
    assert_eq!(status, 0);
    assert!(!standard.contains("definitely/missing"), "{standard}");
    assert!(
        standard.to_ascii_lowercase().contains("where.exe"),
        "{standard}"
    );
}

#[test]
fn bash_52_startup_file_temporarily_sets_zero_and_accepts_windows_paths() {
    let root = source_fixture("startup-zero");
    let startup = root.join("startup.sh");
    std::fs::write(&startup, "printf 'startup=<%s>\\n' \"$0\"\n").unwrap();

    let result = Command::new(CASH)
        .env("BASH_ENV", &startup)
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "printf 'body=<%s>\\n' \"$0\"",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(0));
    let stdout = String::from_utf8(result.stdout)
        .unwrap()
        .replace("\\r\\n", "\\n");
    let startup_name = startup.to_string_lossy().replace('\\', "/");
    assert!(
        stdout.contains(&format!("startup=<{startup_name}>")),
        "{stdout}"
    );
    assert!(stdout.contains("body=<"), "{stdout}");
    assert!(
        !stdout.contains(&format!("body=<{startup_name}>")),
        "{stdout}"
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn bash_52_empty_move_redirection_closes_but_plain_empty_is_an_error() {
    assert_eq!(
        output("exec 3>/dev/null; empty=; { echo x >&3; } 3>&$empty-; echo rc=$?"),
        (0, "rc=1".into())
    );
    assert_eq!(
        output("empty=; : 3>&$empty; echo rc=$?"),
        (0, "rc=1".into())
    );
}

#[test]
fn bash_52_invalid_parameter_transform_is_fatal_noninteractive() {
    let (status, stdout) = output("v=x; echo before; echo ${v@}; echo after");
    assert_eq!(status, 127);
    assert_eq!(stdout, "before");

    let (status, stdout) = output("v=x; echo before; echo ${v@invalid}; echo after");
    assert_eq!(status, 127);
    assert_eq!(stdout, "before");

    assert_eq!(
        output("unset v; printf '<%s>\\n' \"${v@}\"; echo after"),
        (0, "<>\nafter".into())
    );
}

#[test]
fn bash_52_associative_unset_expands_a_quoted_subscript_once() {
    let script = "declare -A a; key='$(echo should-not-run)'; a[$key]=value; unset 'a[$key]'; printf '%s\\n' \"${#a[@]}\"";
    assert_eq!(output(script), (0, "0".into()));

    // With the option (Bash 5.3 also spells it `array_expand_once`), `unset` does not
    // expand the subscript again: a single-quoted `$key` names the literal key, and the
    // double-quoted form, expanded once as an argument, names the real one.
    for option in ["assoc_expand_once", "array_expand_once"] {
        let literal = format!(
            "declare -A a; shopt -s {option}; key='literal key'; a[$key]=value; unset 'a[$key]'; printf '%s\\n' \"${{#a[@]}}\""
        );
        assert_eq!(output(&literal), (0, "1".into()), "{option}");
        let expanded = format!(
            "declare -A a; shopt -s {option}; key='$(echo ran >&2)x'; a[$key]=value; unset \"a[$key]\"; printf '%s\\n' \"${{#a[@]}}\""
        );
        assert_eq!(output(&expanded), (0, "0".into()), "{option}");
    }
}

#[test]
fn bash_53_array_expand_once_is_a_synonym_for_assoc_expand_once() {
    assert_eq!(
        output(
            "shopt -s assoc_expand_once; shopt -q array_expand_once; echo on=$?; shopt -u array_expand_once; shopt -q assoc_expand_once; echo off=$?"
        ),
        (0, "on=0\noff=1".into())
    );
}

#[test]
fn bash_52_nameref_to_all_array_elements_works_with_nounset() {
    let script = "set -u; unset v; f() { local -n r='v[@]'; printf 'unset=<%s>\\n' \"${r-}\"; }; f; v=(one two); declare -n r='v[@]'; printf 'set=<%s>\\n' \"$r\"";
    assert_eq!(output(script), (0, "unset=<>\nset=<one>\nset=<two>".into()));

    let chained = "set -u; v=(one two); declare -n tail='v[*]'; declare -n head=tail; printf '<%s>\\n' \"$head\"";
    assert_eq!(output(chained), (0, "<one two>".into()));
}

#[test]
fn bash_52_builtin_array_subscripts_are_evaluated_once() {
    let script = "i=0; a=(x); test -v 'a[i++]'; printf 'test=%s i=%s\\n' \"$?\" \"$i\"; i=0; a=(); printf -v 'a[i++]' '%s' y; printf 'printf=%s:%s\\n' \"$i\" \"${a[0]}\"; i=0; a=(); read 'a[i++]' <<< z; printf 'read=%s:%s\\n' \"$i\" \"${a[0]}\"";
    assert_eq!(
        output(script),
        (0, "test=0 i=1\nprintf=1:y\nread=1:z".into())
    );
}

#[test]
fn bash_52_posix_mode_enables_and_restores_alias_expansion() {
    assert_eq!(
        output(
            "shopt -u expand_aliases; set -o posix; alias hi='echo ok'; eval 'echo $(hi)'; set +o posix; shopt -q expand_aliases; echo restored=$?"
        ),
        (0, "ok\nrestored=1".into())
    );
}

#[test]
fn bash_52_variable_fd_redirections_and_varredir_close() {
    let root = source_fixture("variable-fd");
    let output_path = root.join("persistent.txt");
    let auto_path = root.join("automatic.txt");
    let exec_path = root.join("exec.txt");
    let array_path = root.join("array.txt");
    let input_path = root.join("input.txt");
    std::fs::write(&input_path, "from-input\n").unwrap();

    let slash = |path: &Path| path.to_string_lossy().replace('\\', "/");
    let script = format!(
        concat!(
            ": {{out}}>\"{}\"; printf 'fd=%s\\n' \"$out\"; ",
            "printf 'kept' >&$out; {{out}}>&-; cat \"{}\"; ",
            ": {{input}}<\"{}\"; IFS= read -r -u \"$input\" value; ",
            "printf ':read=%s\\n' \"$value\"; {{input}}<&-; ",
            "shopt -s varredir_close; : {{auto}}>\"{}\"; ",
            "printf ignored >&$auto 2>/dev/null; printf 'auto=%s\\n' \"$?\"; ",
            "exec {{saved}}>\"{}\"; printf 'exec-kept' >&$saved; {{saved}}>&-; cat \"{}\"; ",
            "shopt -u varredir_close; : {{slot[2]}}>\"{}\"; printf '+array' >&${{slot[2]}}; ",
            "closer=-; {{slot[2]}}>&$closer; cat \"{}\""
        ),
        slash(&output_path),
        slash(&output_path),
        slash(&input_path),
        slash(&auto_path),
        slash(&exec_path),
        slash(&exec_path),
        slash(&array_path),
        slash(&array_path),
    );
    let (status, stdout) = output(&script);
    assert_eq!(status, 0);
    let mut lines = stdout.lines();
    let descriptor = lines.next().unwrap().strip_prefix("fd=").unwrap();
    assert!(descriptor.parse::<i32>().is_ok_and(|fd| fd >= 10));
    assert_eq!(
        lines.collect::<Vec<_>>(),
        ["kept:read=from-input", "auto=1", "exec-kept+array"]
    );

    std::fs::remove_dir_all(root).unwrap();
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

#[test]
fn bash_53_bash_monoseconds_is_monotonic() {
    let (code, stdout) = output(
        "s1=$BASH_MONOSECONDS; [[ $s1 =~ ^[0-9]+$ ]] && echo numeric; [[ $s1 -ge 0 ]] && echo positive",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "numeric\npositive");
}

#[test]
fn bash_53_bash_trapsig_and_trap_p() {
    // trap -P validation
    let (code, stderr) = {
        let res = Command::new(CASH)
            .args(["--noprofile", "--norc", "-c", "trap -P"])
            .output()
            .unwrap();
        (
            res.status.code().unwrap_or(-1),
            String::from_utf8(res.stderr).unwrap().replace("\r\n", "\n"),
        )
    };
    assert_eq!(code, 2);
    assert!(stderr.contains("signal argument required"), "{stderr}");

    // trap -p and -P collision
    let (code, stderr) = {
        let res = Command::new(CASH)
            .args(["--noprofile", "--norc", "-c", "trap -p -P EXIT"])
            .output()
            .unwrap();
        (
            res.status.code().unwrap_or(-1),
            String::from_utf8(res.stderr).unwrap().replace("\r\n", "\n"),
        )
    };
    assert_eq!(code, 2);
    assert!(stderr.contains("cannot specify both"), "{stderr}");

    // trap -P prints only action
    assert_eq!(
        output("trap 'echo exiting' EXIT; trap -P EXIT; trap -p EXIT; trap - EXIT"),
        (0, "echo exiting\ntrap -- 'echo exiting' EXIT".into())
    );

    // BASH_TRAPSIG inside trap handler
    assert_eq!(
        output("trap 'echo sig=$BASH_TRAPSIG' EXIT; exit"),
        (0, "sig=0".into())
    );
}

#[test]
fn bash_53_empty_path_resolves_to_current_directory() {
    let fixture = source_fixture("empty-path");
    let script_file = fixture.join("hello.bat");
    std::fs::write(&script_file, "@echo hello from bat\r\n").unwrap();

    let (code, stdout) = output_in(&fixture, "PATH= hello; PATH='' hello; PATH=':' hello");
    assert_eq!(code, 0);
    assert_eq!(stdout, "hello from bat\nhello from bat\nhello from bat");

    let _ = std::fs::remove_dir_all(&fixture);
}

fn output_with_stderr(script: &str) -> (i32, String, String) {
    let result = Command::new(CASH)
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let text = |bytes: Vec<u8>| {
        String::from_utf8(bytes)
            .unwrap()
            .replace("\r\n", "\n")
            .trim_end()
            .to_string()
    };
    (
        result.status.code().unwrap_or(-1),
        text(result.stdout),
        text(result.stderr),
    )
}

fn run_script_file(path: &Path) -> (i32, String, String) {
    let result = Command::new(CASH)
        .args(["--noprofile", "--norc"])
        .arg(path)
        .output()
        .unwrap();
    let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    (
        result.status.code().unwrap_or(-1),
        text(result.stdout),
        text(result.stderr),
    )
}

#[test]
fn bash_53_relative_path_entries_follow_the_shell_working_directory() {
    // `cd` changes the shell's directory, not the process's; `.` and empty entries in
    // `$PATH` must be searched relative to the former, and the hit must not be cached.
    let fixture = source_fixture("relative-path");
    std::fs::create_dir_all(fixture.join("bin")).unwrap();
    std::fs::write(fixture.join("bin").join("tool"), "#!/bin/sh\necho ran\n").unwrap();

    let (code, stdout) = output_in(
        &fixture,
        "cd bin; PATH= tool; PATH=. tool; PATH=: tool; cd ..; PATH=. tool; echo rc=$?",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "ran\nran\nran\nrc=127");

    let _ = std::fs::remove_dir_all(&fixture);
}

#[test]
fn exit_and_return_in_err_and_debug_traps_take_effect() {
    // `exit` in an ERR trap ends the shell with the failing status.
    assert_eq!(
        output("trap exit ERR; (exit 6); echo unreachable"),
        (6, String::new())
    );
    assert_eq!(
        output("trap 'exit 3' ERR; false; echo unreachable"),
        (3, String::new())
    );
    // `exit` in a DEBUG trap stops before the command runs.
    assert_eq!(
        output("trap 'exit 4' DEBUG; echo unreachable"),
        (4, String::new())
    );
    // `return` in an ERR trap set inside a function returns from that function.
    assert_eq!(
        output("h() { trap 'echo in-h; return 9' ERR; false; echo notreached; }; h; echo h=$?"),
        (0, "in-h\nh=9".into())
    );
}

#[test]
fn err_trap_scoping_follows_bash_function_rules() {
    // Without errtrace a function does not see the caller's ERR trap, but a trap the
    // function sets itself fires there and survives the return.
    let (_, stdout) = output(
        "trap 'echo top' ERR; f() { trap -p ERR; false; echo f-after; }; f; set -E; f; set +E; trap - ERR; g() { trap 'echo g-own' ERR; }; g; false",
    );
    assert_eq!(
        stdout,
        "f-after\ntrap -- 'echo top' ERR\ntop\nf-after\ng-own"
    );
}

#[test]
fn return_trap_fires_for_source_traced_functions_and_functrace() {
    let root = source_fixture("return-trap");
    std::fs::write(root.join("rt.sh"), "echo in\n").unwrap();
    let (code, stdout) = output_in(
        &root,
        "trap 'echo ret' RETURN; . ./rt.sh; f() { :; }; f; declare -ft f; f; declare +ft f; f; g() { :; }; set -T; g",
    );
    assert_eq!(code, 0);
    // `declare +ft f` does not clear the function's attribute: as in Bash, `+f` is
    // ignored and the `+t` applies to a variable named `f`.
    assert_eq!(stdout, "in\nret\nret\nret\nret");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bash_53_trapsig_numbers_each_trap_and_is_unset_outside_one() {
    assert_eq!(
        output(
            "echo out=${BASH_TRAPSIG-unset}; trap 'echo err=$BASH_TRAPSIG' ERR; false; trap - ERR; trap 'echo dbg=$BASH_TRAPSIG; trap - DEBUG' DEBUG; :; trap 'echo exit=$BASH_TRAPSIG' EXIT"
        ),
        (0, "out=unset\nerr=66\ndbg=65\nexit=0".into())
    );
}

#[test]
fn bash_53_printf_alternate_q_forces_single_quotes() {
    assert_eq!(
        output("printf '<%#q>\\n' abc 'a b' \"it's\" '' $'a\\tb'; printf '<%q>\\n' abc"),
        (
            0,
            "<'abc'>\n<'a b'>\n<'it'\\''s'>\n<''>\n<$'a\\tb'>\n<abc>".into()
        )
    );
}

#[test]
fn declare_and_export_accept_assignments_in_ordinary_words() {
    assert_eq!(
        output(
            "declare \"a=x y\"; v='b=p q'; declare $v; declare \"c[1]=z\"; declare \"n+=1\"; declare \"n+=2\"; printf '<%s>' \"$a\" \"$b\" \"${c[1]}\" \"$n\"; echo"
        ),
        (0, "<x y><p><z><12>".into())
    );
    assert_eq!(
        output(
            "export \"X=1 2\"; v='W=6 7'; export \"$v\"; export \"W+=8\"; printf '<%s>' \"$X\" \"$W\"; env | grep -c '^X=1 2$'"
        ),
        (0, "<1 2><6 78>1".into())
    );
}

#[test]
fn bash_53_posix_mode_command_keeps_declaration_assignment_parsing() {
    assert_eq!(
        output(
            "set -o posix; command declare d=$(echo 1 2); command command export e=$(echo 3 4); printf '<%s>' \"$d\" \"$e\"; echo"
        ),
        (0, "<1 2><3 4>".into())
    );
    // Outside POSIX mode Bash 5.3 still splits the expansion.
    assert_eq!(
        output("command declare d=$(echo 5 6) 2>/dev/null; printf '<%s>\\n' \"$d\""),
        (0, "<5>".into())
    );
}

#[test]
fn bash_53_timeformat_controls_the_time_report() {
    let (code, stdout, stderr) = output_with_stderr(
        "TIMEFORMAT='%6R|%3R|%0R|%9R|%2lR|%%|x'; time :; TIMEFORMAT=; time :; TIMEFORMAT='%Q'; time :; echo done",
    );
    assert_eq!((code, stdout.as_str()), (0, "done"));
    let lines: Vec<&str> = stderr.lines().collect();
    assert_eq!(lines.len(), 2, "{stderr}");
    let fields: Vec<&str> = lines[0].split('|').collect();
    assert_eq!(fields.len(), 7, "{stderr}");
    let decimals = |f: &str| f.split_once('.').map_or(0, |(_, d)| d.len());
    assert_eq!(decimals(fields[0]), 6);
    assert_eq!(decimals(fields[1]), 3);
    assert!(!fields[2].contains('.'), "{stderr}");
    assert_eq!(decimals(fields[3]), 6, "precision is capped at 6");
    assert!(
        fields[4].starts_with("0m") && fields[4].ends_with('s'),
        "{stderr}"
    );
    assert_eq!(&fields[5..], ["%", "x"]);
    assert!(
        lines[1].contains("TIMEFORMAT: `Q': invalid format character"),
        "{stderr}"
    );

    let (_, _, posix) = output_with_stderr("time -p :");
    assert!(
        posix.starts_with("real 0.") && posix.contains("\nuser ") && posix.contains("\nsys "),
        "{posix}"
    );
}

#[test]
fn bash_53_globsort_orders_pathname_expansion() {
    let root = source_fixture("globsort");
    std::fs::write(root.join("b"), "aaa").unwrap();
    std::fs::write(root.join("c"), "a").unwrap();
    std::fs::write(root.join("a"), "aa").unwrap();
    let (code, stdout) = output_in(
        &root,
        "echo *; GLOBSORT=-name; echo *; GLOBSORT=size; echo *; GLOBSORT=-size; echo *; GLOBSORT=bogus; echo *; touch -t 202001010000 a; touch -t 202101010000 b; touch -t 201901010000 c; GLOBSORT=mtime; echo *; GLOBSORT=-mtime; echo *",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "a b c\nc b a\nc a b\nb a c\na b c\nc a b\nb a c");
    let _ = std::fs::remove_dir_all(&root);

    let numeric = source_fixture("globsort-numeric");
    for name in ["10", "9", "100", "1"] {
        std::fs::write(numeric.join(name), "").unwrap();
    }
    assert_eq!(
        output_in(
            &numeric,
            "echo *; GLOBSORT=numeric; echo *; GLOBSORT=-numeric; echo *"
        ),
        (0, "1 10 100 9\n1 9 10 100\n100 10 9 1".into())
    );
    let _ = std::fs::remove_dir_all(&numeric);
}

#[test]
fn bash_53_bash_source_fullpath_records_the_real_path() {
    let root = source_fixture("source-fullpath");
    std::fs::write(root.join("src.sh"), "echo \"${BASH_SOURCE[0]}\"\n").unwrap();
    let (code, stdout) = output_in(
        &root,
        ". ./src.sh; shopt -s bash_source_fullpath; . ./src.sh",
    );
    assert_eq!(code, 0);
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(lines[0], "./src.sh");
    let expected = std::fs::canonicalize(root.join("src.sh")).unwrap();
    let expected = expected.to_string_lossy().replace('\\', "/");
    let expected = expected.trim_start_matches("//?/");
    assert!(
        lines[1].eq_ignore_ascii_case(expected),
        "{stdout} vs {expected}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bash_53_regex_compile_error_is_reported_with_status_2() {
    let (code, stdout, stderr) =
        output_with_stderr("re='('; [[ a =~ $re ]]; echo rc=$?; [[ a =~ \"(\" ]]; echo literal=$?");
    assert_eq!((code, stdout.as_str()), (0, "rc=2\nliteral=1"));
    assert!(stderr.contains("invalid regular expression"), "{stderr}");
}

#[test]
fn bash_53_unterminated_compound_names_its_starting_line() {
    let root = source_fixture("unterminated");
    for (name, body, keyword, line) in [
        ("if.sh", "echo a\n\nif true; then\n echo b\n", "if", 3),
        ("while.sh", "echo a\nwhile true; do\n echo b\n", "while", 2),
        (
            "brace.sh",
            "f() {\n  case x in\n   x) echo;;\n  esac\n",
            "{",
            1,
        ),
        (
            "nested.sh",
            "if true; then\n  for i in a; do\n   echo\n  done\n",
            "if",
            1,
        ),
    ] {
        let path = root.join(name);
        std::fs::write(&path, body).unwrap();
        let (code, _, stderr) = run_script_file(&path);
        assert_ne!(code, 0, "{name}");
        assert!(
            stderr.contains(&format!(
                "unexpected end of file from `{keyword}' command on line {line}"
            )),
            "{name}: {stderr}"
        );
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn bash_53_binary_script_check_and_nul_bytes() {
    let root = source_fixture("binary-script");
    // With `#!`, only the first two lines are checked; later NULs are discarded input.
    let shebang = root.join("late-nul.sh");
    std::fs::write(&shebang, b"#!/bin/sh\necho ran\n\0\necho two\n").unwrap();
    assert_eq!(
        run_script_file(&shebang),
        (0, "ran\ntwo\n".into(), String::new())
    );

    for (name, body) in [
        ("first-line.sh", &b"echo ran\0x\necho two\n"[..]),
        ("second-line.sh", &b"#!/bin/sh\necho a\0b\necho two\n"[..]),
        ("elf.sh", &b"\x7fELF\necho no\n"[..]),
    ] {
        let path = root.join(name);
        std::fs::write(&path, body).unwrap();
        let (code, stdout, stderr) = run_script_file(&path);
        assert_eq!(code, 126, "{name}");
        assert!(stdout.is_empty(), "{name}: {stdout}");
        assert!(
            stderr.contains("cannot execute binary file"),
            "{name}: {stderr}"
        );
    }

    // `source` does not check, and still drops the NUL.
    assert_eq!(
        output_in(&root, ". ./first-line.sh"),
        (0, "ranx\ntwo".into())
    );

    let (code, _, _) = run_script_file(&root.join("does-not-exist.sh"));
    assert_eq!(code, 127);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn read_last_variable_drops_a_lone_trailing_delimiter() {
    // Found by the corpus (Wooledge BashPitfalls); expected values from Bash 5.3.
    assert_eq!(
        output(
            "for s in foo:bar: x:y:z: x:: ; do IFS=: read -r a b <<< \"$s\"; printf \"[%s][%s]\" \"$a\" \"$b\"; done; echo"
        ),
        (0, "[foo][bar][x][y:z:][x][]".into())
    );
}

#[test]
fn quoted_prefix_name_expansion_with_at() {
    // `"${!prefix@}"` was parsed as an invalid `@` transform and ended the script.
    assert_eq!(
        output(
            "hello_a=1 hello_b=2; v=hello_a; printf \"<%s>\" \"${!hello_@}\" \"${!hello_*}\" \"${!v@Q}\"; echo"
        ),
        (0, "<hello_a><hello_b><hello_a hello_b><'1'>".into())
    );
}

#[test]
fn v_test_expands_an_associative_subscript() {
    // BashPitfalls pf61: the single-quoted subscript is expanded by `-v` itself. As in
    // Bash, `test -v` honours `assoc_expand_once` and `[[ -v` does not.
    assert_eq!(
        output(
            "declare -A h; h[\"x y\"]=1; key=\"x y\"; [[ -v 'h[$key]' ]] && echo dbl; test -v 'h[$key]' && echo test; shopt -s assoc_expand_once; [[ -v 'h[$key]' ]] && echo dbl-once; test -v 'h[$key]' || echo test-once-literal"
        ),
        (0, "dbl\ntest\ndbl-once\ntest-once-literal".into())
    );
}

#[test]
fn printf_zero_flag_pads_strings_with_zeros() {
    // pure-bash-bible's progress bar uses `printf -v total "%0s"`; uucore rejected the `0`
    // flag on `%s`. Expected values from Bash 5.3.
    assert_eq!(
        output(
            "printf '[%0s][%05s][%-05s][%00s][%0.0s][%07.2s]' a b c d e fgh; printf -v t '%0s'; echo \" t=[$t]\""
        ),
        (0, "[a][0000b][c    ][d][][00000fg] t=[]".into())
    );
}

#[test]
fn for_and_select_accept_a_brace_group_body() {
    // pure-bash-bible's code-golf loops; Bash accepts these even in POSIX mode.
    assert_eq!(
        output(
            "for i in {1..2};{ echo $i;}; for i in a; { echo $i; }; set -- p q; for x; { echo $x; }"
        ),
        (0, "1\n2\na\np\nq".into())
    );
}

#[test]
fn for_with_an_empty_in_list_runs_zero_times() {
    // `in` with no words is an empty list; only a missing `in` means "$@".
    assert_eq!(
        output("set -- p q; for x in; do echo for:$x; done; for x; do echo bare:$x; done"),
        (0, "bare:p\nbare:q".into())
    );
}

#[test]
fn at_a_transformation_keeps_scalar_attributes() {
    // BashFAQ/073: `${var@A}` of a scalar with attributes is a `declare` command.
    assert_eq!(
        output(
            "declare -ri i=3; declare -rx rx=2; f=plain; printf '%s\n' \"${i@A}\" \"${rx@A}\" \"${f@A}\""
        ),
        (0, "declare -ir i='3'\ndeclare -rx rx='2'\nf='plain'".into())
    );
}

#[test]
fn arithmetic_uses_associative_subscripts_as_keys() {
    // The word-count idiom: `(( count[$word]++ ))` put every word under key 0, because
    // the subscript was evaluated as arithmetic. BashPitfalls pf62 covers the `let` form.
    assert_eq!(
        output(concat!(
            "declare -A c; for w in apple pear apple; do (( c[$w]++ )); done; ",
            "key='a b'; let 'c[$key]++'; let 'c[${key}]+=2'; ",
            "echo \"${c[apple]} ${c[pear]} ${c[a b]} ${#c[@]}\"; ",
            "declare -a n=(5 6 7); i=1; echo $(( n[i+1] )) $(( n[$i] ))"
        )),
        (0, "2 1 3 3\n7 6".into())
    );
}

#[test]
fn arithmetic_never_runs_command_substitutions_from_variable_values() {
    // BashPitfalls: `read num; echo $((num+1))` with `a[$(cmd)]` as input runs `cmd` in
    // Bash. Cash refuses deliberately (spec §4): an indexed subscript is an arithmetic
    // error, an associative one is a literal key; neither executes anything.
    let (code, _, stderr) = output_with_stderr("num='a[$(echo INJECTED >&2)]'; echo $((num+1))");
    assert_ne!(code, 0);
    // The error message may quote the expression; only a line of its own is output.
    assert!(!stderr.lines().any(|l| l == "INJECTED"), "{stderr}");

    let (code, stdout, stderr) =
        output_with_stderr("declare -A a; num='a[$(echo INJECTED >&2)]'; echo $((num+1))");
    assert_eq!((code, stdout.as_str()), (0, "1"));
    // The error message may quote the expression; only a line of its own is output.
    assert!(!stderr.lines().any(|l| l == "INJECTED"), "{stderr}");
}
