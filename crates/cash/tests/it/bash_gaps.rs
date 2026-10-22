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

use crate::common::{CASH, Scratch, cash_command};

fn output(script: &str) -> (i32, String) {
    let result = cash_command()
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
    let result = cash_command()
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

fn source_fixture(name: &str) -> Scratch {
    Scratch::new(&format!("source-{name}"))
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
}

#[test]
fn source_takes_options_only_before_the_script() {
    // As Bash 5.3: what follows the script's path is the script's, `--help` and `--`
    // included; a missing path is said, and the usage names `.` itself.
    let root = source_fixture("options");
    std::fs::write(root.join("args.sh"), "echo \"args: $*\"\n").unwrap();
    let (code, stdout) = output_in(
        root.path(),
        ". ./args.sh --help -x; . ./args.sh -- a; source ./args.sh -p q; . -p . args.sh one; \
         . 2>/dev/null; echo \"st $?\"; . 2>&1 | grep -c ': filename argument required$'; \
         . 2>&1 | grep -x '.: usage: . \\[-p path\\] filename \\[arguments\\]'; \
         . --help | sed -n 3p",
    );
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "args: --help -x\nargs: -- a\nargs: -p q\nargs: one\nst 2\n1\n\
         .: usage: . [-p path] filename [arguments]\n\
         Usage: . [OPTIONS] <SCRIPT_PATH [ARGUMENTS]>..."
    );
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
            root.path(),
            "set -- outer; source ./mutate.sh inner; printf 'after=<%s>\\n' \"$*\"",
        ),
        (
            0,
            "inside=<changed by source>\nafter=<changed by source>".into()
        )
    );
    assert_eq!(
        output_in(
            root.path(),
            "f() { set -- function outer; source ./mutate.sh inner; printf 'function-after=<%s>\\n' \"$*\"; }; f",
        ),
        (
            0,
            "inside=<changed by source>\nfunction-after=<function outer>".into()
        )
    );
    assert_eq!(
        output_in(
            root.path(),
            "set -- outer; source ./empty.sh inner; printf 'after-count=%s\\n' \"$#\"",
        ),
        (0, "inside-count=0\nafter-count=0".into())
    );
    assert_eq!(
        output_in(
            root.path(),
            "set -- outer; source ./shift.sh first second; printf 'after-shift=<%s>\\n' \"$*\"",
        ),
        (0, "shifted=<second>\nafter-shift=<outer>".into())
    );
}

#[test]
fn source_inherits_debug_trap_only_with_functrace() {
    let root = source_fixture("debug-trap");
    std::fs::write(root.join("body.sh"), "printf 'inside\\n'\n").unwrap();

    assert_eq!(
        output_in(
            root.path(),
            "trap 'case $BASH_SOURCE in *body.sh) echo inherited-debug;; esac' DEBUG; source ./body.sh; set -T; source ./body.sh",
        ),
        (0, "inside\ninherited-debug\ninside".into())
    );
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
    // The second job ends a second before the first: 150 ms apart, a busy machine had
    // started them too far apart for that order to hold (2026-09-30).
    let (_, first) = output(
        "bash -c 'sleep 1; exit 3' & a=$!; bash -c 'exit 7' & b=$!; wait -n $a $b; echo $?; wait $a; echo $?",
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
    let root = Scratch::new("bash-env");
    let startup = root.join("startup.sh");
    std::fs::write(&startup, "echo startup\n").unwrap();
    let run = |posix| {
        // The test's own `BASH_ENV`, in place of the developer's `cash_command()` removed.
        let mut command = cash_command();
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
    let (_, normal) = output("hash -p C:/definitely/missing/cmd.exe cmd; command -v cmd");
    assert!(normal.contains("definitely/missing"), "{normal}");

    let (status, standard) = output("hash -p C:/definitely/missing/cmd.exe cmd; command -p -v cmd");
    assert_eq!(status, 0);
    assert!(!standard.contains("definitely/missing"), "{standard}");
    assert!(
        standard.to_ascii_lowercase().contains("cmd.exe"),
        "{standard}"
    );
}

/// The value between `label=<` and `>` on a line of `stdout`.
fn labelled(stdout: &str, label: &str) -> Option<String> {
    stdout.lines().find_map(|line| {
        let rest = line.strip_prefix(label)?.strip_prefix("=<")?;
        Some(rest.strip_suffix('>')?.to_owned())
    })
}

/// `$0` stays the shell's name while a startup file runs: Git Bash 5.3.15 keeps it for
/// `$BASH_ENV`, `~/.bashrc` and `~/.bash_profile` alike (checked 2026-09-29). cash named
/// the file, after a reading of Bash 5.2's notes that bash does not bear out. A
/// `$BASH_ENV` spelled with backslashes, as Windows spells it, still runs.
#[test]
fn startup_file_keeps_the_shells_zero_and_accepts_windows_paths() {
    let root = source_fixture("startup-zero");
    let startup = root.join("startup.sh");
    std::fs::write(&startup, "printf 'startup=<%s>\\n' \"$0\"\n").unwrap();

    // The test's own `BASH_ENV`, in place of the developer's `cash_command()` removed.
    let result = cash_command()
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
    let stdout = String::from_utf8(result.stdout).unwrap();
    let in_startup = labelled(&stdout, "startup");
    assert!(
        in_startup.is_some(),
        "the startup file did not run: {stdout}"
    );
    assert_eq!(in_startup, labelled(&stdout, "body"), "{stdout}");
}

/// In `~/.bashrc` too, `$0` is the shell's name, and `BASH_SOURCE` names the file as cash
/// spells paths (D3), not with the backslash joining it onto the home directory left.
/// `coolfetch`, the banner a `~/.bashrc` runs, called the shell `.bashrc`.
#[test]
fn bashrc_sees_the_shells_zero_and_its_own_name_with_forward_slashes() {
    let home = source_fixture("bashrc-zero");
    std::fs::write(
        home.join(".bashrc"),
        "printf 'rc=<%s>\\n' \"$0\"\nprintf 'source=<%s>\\n' \"$BASH_SOURCE\"\n\
         coolfetch --no-color --no-logo | grep 'Shell:'\n",
    )
    .unwrap();

    let result = cash_command()
        .env("HOME", home.path())
        .args(["--noprofile", "-i", "-c", "printf 'body=<%s>\\n' \"$0\""])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    let in_rc = labelled(&stdout, "rc");
    assert!(in_rc.is_some(), "~/.bashrc did not run: {stdout}");
    assert_eq!(in_rc, labelled(&stdout, "body"), "{stdout}");

    let home_name = home.as_script_path();
    assert_eq!(
        labelled(&stdout, "source"),
        Some(format!("{home_name}/.bashrc")),
        "{stdout}"
    );
    assert!(
        stdout.lines().any(|line| line.contains("Shell: cash ")),
        "coolfetch did not call the shell cash: {stdout}"
    );
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
fn mapfile_keeps_control_bytes_of_input_that_is_not_a_terminal() {
    // Ctrl-C and Ctrl-D are keys at a terminal, and data in a pipe or a file: a 0x03
    // ended mapfile's input, and so did a 0x04 where a line started.
    assert_eq!(
        output(
            "printf 'a\\003b\\n\\004d\\n' | { mapfile -t x; printf '%q\\n' \"${#x[@]}\" \"${x[@]}\"; }"
        ),
        (0, "2\n$'a\\003b'\n$'\\004d'".into())
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
    let root = Scratch::new("globskipdots");
    std::fs::write(root.join(".hidden"), "").unwrap();
    let path = root.as_script_path();
    let result = output(&format!(
        "cd '{path}'; shopt -u globskipdots; printf '<%s>\\n' .*"
    ));
    assert_eq!(result, (0, "<.>\n<..>\n<.hidden>".into()));
}

#[test]
fn globignore_and_a_last_globstar_match_as_in_bash() {
    // As Bash 5.3: GLOBIGNORE takes out what its patterns match, a part of a pattern for
    // each part of a path but a last `*` taking any parts after, never `.` or `..`, and lets
    // a glob find dot files; a last `**` matches its directory too, `sub/` as written, `sub`
    // as a pattern found it, and `**/` leaves the working directory out.
    let root = Scratch::new("globignore");
    for directory in ["s", "sub/deep"] {
        std::fs::create_dir_all(root.join(directory)).unwrap();
    }
    for file in [
        ".hid",
        "a.log",
        "a.txt",
        "s/b.log",
        "s/c.txt",
        "sub/d.txt",
        "sub/deep/e.txt",
    ] {
        std::fs::write(root.join(file), "").unwrap();
    }
    let path = root.as_script_path();
    let (code, stdout) = output(&format!(
        "cd '{path}'; for p in '*.log' 's*' '*/b.log' '?/*' 's/*' '.*:*.txt'; do \
         GLOBIGNORE=$p; echo \"[$p]\" * s/*; done; GLOBIGNORE=a.txt; echo a.txt a.tx?; \
         unset GLOBIGNORE; echo *; shopt -s globstar; echo sub/**; echo ./s/**; echo */**; \
         echo **/"
    ));
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "[*.log] .hid a.txt s sub s/b.log s/c.txt\n\
         [s*] .hid a.log a.txt s/*\n\
         [*/b.log] .hid a.log a.txt s sub s/c.txt\n\
         [?/*] .hid a.log a.txt s sub s/*\n\
         [s/*] .hid a.log a.txt s sub s/*\n\
         [.*:*.txt] a.log s sub s/b.log s/c.txt\n\
         a.txt a.tx?\n\
         a.log a.txt s sub\n\
         sub/ sub/d.txt sub/deep sub/deep/e.txt\n\
         ./s/ ./s/b.log ./s/c.txt\n\
         s s/b.log s/c.txt sub sub/d.txt sub/deep sub/deep/e.txt\n\
         s/ sub/ sub/deep/"
    );
}

#[test]
fn line_numbers_stay_right_across_a_pipeline_and_a_second_parse() {
    // As Bash 5.3: an error in a pipeline's stage names the pipeline's line, where it
    // named the line of the command before; and a text parsed again from a command on,
    // after `shopt -s extglob` changed how it parses, keeps its lines.
    let (code, stdout, stderr) = output_with_stderr(
        "echo one\nno_such_b | cat\nshopt -s extglob\necho \"line $LINENO\"\nf() {\n  echo \"f $LINENO\"\n}\nf\nno_such_c | cat",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "one\nline 4\nf 6");
    assert!(
        stderr.contains("line 2: no_such_b: command not found"),
        "{stderr}"
    );
    assert!(
        stderr.contains("line 9: no_such_c: command not found"),
        "{stderr}"
    );
}

#[test]
fn globignore_turns_dotglob_on_and_its_unset_turns_it_off() {
    // As Bash 5.3: patterns in GLOBIGNORE turn dotglob on, an empty one leaves it, and
    // unsetting it turns dotglob off.
    assert_eq!(
        output(
            "shopt -q dotglob; echo $?; GLOBIGNORE=x; shopt -q dotglob; echo $?; \
             GLOBIGNORE=; shopt -q dotglob; echo $?; unset GLOBIGNORE; shopt -q dotglob; echo $?"
        ),
        (0, "1\n0\n0\n1".into())
    );
}

#[test]
fn associative_unset_at_and_star_are_literal_keys() {
    assert_eq!(
        output("declare -A a=([@]=at [*]=star [x]=other); unset 'a[@]'; declare -p a"),
        (0, "declare -A a=([\"*\"]=\"star\" [x]=\"other\" )".into())
    );
    assert_eq!(
        output("declare -A a=([@]=at [*]=star [x]=other); unset 'a[*]'; declare -p a"),
        (0, "declare -A a=([\"@\"]=\"at\" [x]=\"other\" )".into())
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
    let root = Scratch::new("history-gaps");
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
fn a_trap_sees_the_command_it_came_at_in_bash_command() {
    // As Bash: inside a trap, `$BASH_COMMAND` is the command the trap came at, which a
    // DEBUG trap is there to show; it was the trap's own `echo`. Assigning it changes
    // nothing it says, and unsetting it ends it.
    assert_eq!(
        output(
            "trap 'echo \"[$BASH_COMMAND]\"' DEBUG; echo hi; trap - DEBUG; \
             trap 'echo \"exit after: $BASH_COMMAND\"' EXIT; BASH_COMMAND=x; echo \"$BASH_COMMAND\"; false"
        ),
        (
            1,
            "[echo hi]\nhi\n[trap - DEBUG]\necho \"$BASH_COMMAND\"\nexit after: false".into()
        )
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
        let res = cash_command()
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
        let res = cash_command()
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
    // As Bash 5.3 lists them: in signal order with EXIT first and DEBUG, ERR and RETURN
    // last, a signal with its `SIG` prefix, a quote in the command quoted again.
    assert_eq!(
        output(
            "trap 'echo r' RETURN; trap 'echo it'\\''s' ERR; trap 'echo t' TERM; trap '' 2; \
             trap 'echo e' EXIT; trap 'echo h' SIGHUP; trap -p; trap -p SIGINT ERR 0; \
             trap - RETURN ERR TERM INT EXIT HUP"
        ),
        (
            0,
            "trap -- 'echo e' EXIT\ntrap -- 'echo h' SIGHUP\ntrap -- '' SIGINT\n\
             trap -- 'echo t' SIGTERM\ntrap -- 'echo it'\\''s' ERR\ntrap -- 'echo r' RETURN\n\
             trap -- '' SIGINT\ntrap -- 'echo it'\\''s' ERR\ntrap -- 'echo e' EXIT"
                .into()
        )
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

    let (code, stdout) = output_in(fixture.path(), "PATH= hello; PATH='' hello; PATH=':' hello");
    assert_eq!(code, 0);
    assert_eq!(stdout, "hello from bat\nhello from bat\nhello from bat");
}

fn output_with_stderr(script: &str) -> (i32, String, String) {
    let result = cash_command()
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
    let result = cash_command()
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
        fixture.path(),
        "cd bin; PATH= tool; PATH=. tool; PATH=: tool; cd ..; PATH=. tool; echo rc=$?",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "ran\nran\nran\nrc=127");
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
fn enable_in_a_subshell_stays_in_it() {
    // A subshell shares its parent's table of builtins and copies it when it changes it:
    // `enable -n` in `( )` or `$( )` is the subshell's alone, as in Bash, and the parent's
    // goes into the subshells it starts after.
    let (code, stdout) = output(
        r#"(enable -n shopt; type -t shopt || echo none); type -t shopt; x=$(enable -n shopt; type -t shopt || echo none); echo "$x"; type -t shopt; enable -n shopt; (type -t shopt || echo none)"#,
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "none\nbuiltin\nnone\nbuiltin\nnone");
}

#[test]
fn a_closed_descriptor_is_said_and_read_as_bash_does() {
    // `echo hi >&-` said "echo: i/o error: I/O write error: standard output not
    // available"; `read` and `mapfile` on a closed standard input said "invalid file
    // descriptor", which Bash says only of a descriptor `-u` names.
    let result = cash_command()
        .args([
            "--noprofile",
            "--norc",
            "-c",
            r#"echo hi >&-; echo "echo $?"; printf x >&-; echo "printf $?"; read x <&-; echo "read $?"; a=(old); mapfile a <&-; echo "mapfile $? ${#a[@]}"; read -u 5 x; echo "read -u $?""#,
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout).replace("\r\n", "\n");
    let stderr = String::from_utf8_lossy(&result.stderr).replace("\r\n", "\n");
    assert_eq!(stdout, "echo 1\nprintf 1\nread 1\nmapfile 0 0\nread -u 1\n");
    let said: Vec<&str> = stderr
        .lines()
        .map(|line| line.split_once(": line 1: ").map_or(line, |(_, said)| said))
        .collect();
    assert_eq!(
        said,
        [
            "echo: write error: Bad file descriptor",
            "printf: write error: Bad file descriptor",
            "read: 0: read error: Bad file descriptor",
            "read: 5: invalid file descriptor: Bad file descriptor",
        ]
    );
}

#[test]
fn debug_trap_runs_before_assignments_and_outside_redirections() {
    // As in Bash 5.2: before every simple command, an assignment alone too, and before
    // the command's words are expanded and its redirections made, so what the trap writes
    // is not the command's. cash ran it only before a command with a name, after the
    // expansion, with the command's `> file` already its output.
    let (code, stdout) = output(
        r#"trap 'echo "trap: $BASH_COMMAND"' DEBUG; x=1; y=$(echo hi) z=2; echo "x=$x y=$y" > /dev/null; trap - DEBUG"#,
    );
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "trap: x=1\ntrap: y=$(echo hi) z=2\ntrap: echo \"x=$x y=$y\" > /dev/null\ntrap: trap - DEBUG"
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
fn a_subshell_runs_the_traps_it_set_itself() {
    // As Bash 5.3: a subshell, a command substitution, a compound pipeline stage and a
    // background job run their own EXIT trap as they end, and an `exit` there sets the
    // status; their own ERR and DEBUG traps run without errtrace or functrace. The
    // parent's EXIT trap, which the copy carries, is not theirs to run.
    let (code, stdout) = output(
        "( trap 'echo own err' ERR; false; echo after ); \
         ( trap 'echo own debug' DEBUG; : one ); \
         x=$( trap 'echo cmdsub exit' EXIT; echo body ); echo \"[$x]\"; \
         { trap 'echo brace exit' EXIT; echo brace; } | cat; trap - EXIT; \
         ( trap 'echo status $?' EXIT; exit 4 ); echo \"st $?\"; \
         ( trap 'exit 7' EXIT; exit 3 ); echo \"st $?\"; \
         ( trap 'echo outer exit' EXIT; ( echo inner ); echo outer ); \
         { trap 'echo job exit' EXIT; echo job; } & wait; \
         trap 'echo parent exit' EXIT; ( echo child ); trap - EXIT; \
         trap 'echo parent err' ERR; ( false; echo no errtrace ); trap - ERR",
    );
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "own err\nafter\nown debug\n[body\ncmdsub exit]\nbrace\nbrace exit\nstatus 4\nst 4\n\
         st 7\ninner\nouter\nouter exit\njob\njob exit\nchild\nno errtrace"
    );
}

#[test]
fn an_alias_is_a_word_as_written_and_expands_as_bash_expands_it() {
    // Any quoting keeps a word from naming an alias, and so does an expansion; an
    // alias's first word is expanded again unless it names one being expanded, and the
    // word after an alias that ends in a blank is expanded too.
    let (_, stdout, stderr) = output_with_stderr(
        "shopt -s expand_aliases\nalias say='echo aliased'\nsay one\n\\say two\n'say' three\n\
         \"say\" four\ns\\ay five\nsa''y six\ncmd=say\n$cmd seven\n\
         alias e1='echo first' e2='e1 second'\ne2 eight\n\
         alias sp='echo spaced '\nsp say nine\n\
         alias ls='ls -d'\nls / >/dev/null && echo stops\nX=1 say ten",
    );
    assert_eq!(
        stdout,
        "aliased one\nfirst second eight\nspaced echo aliased nine\nstops\naliased ten"
    );
    assert_eq!(
        stderr.matches("say: command not found").count(),
        6,
        "{stderr}"
    );
}

#[test]
fn an_alias_is_read_before_its_line_is_parsed() {
    // As Bash 5.3: an alias's text is read in place of its word before the line is
    // parsed, so its `&&` and `;` are operators and a pipe after it takes its last
    // command; it may open a loop; one defined on a line applies from the next; a case
    // pattern is no command; and a function keeps the alias it was defined with.
    let (code, stdout) = output(
        "shopt -s expand_aliases\nalias both='echo first && echo second' up='tr a-z A-Z'\n\
         both | up\nalias loop3='for i in 1 2 3; do'\nloop3 echo \"i=$i\"; done\n\
         alias same='echo same'; same 2>/dev/null || echo not-yet\nsame\n\
         alias say='echo said'\ncase say in say) echo pattern;; esac\nf() { say in-f; }\n\
         alias say='echo changed'\nf\nsay now",
    );
    assert_eq!(code, 0);
    assert_eq!(
        stdout,
        "first\nSECOND\ni=1\ni=2\ni=3\nnot-yet\nsame\npattern\nsaid in-f\nchanged now"
    );
}

#[test]
fn return_trap_fires_for_source_traced_functions_and_functrace() {
    let root = source_fixture("return-trap");
    std::fs::write(root.join("rt.sh"), "echo in\n").unwrap();
    let (code, stdout) = output_in(
        root.path(),
        "trap 'echo ret' RETURN; . ./rt.sh; f() { :; }; f; declare -ft f; f; declare +ft f; f; g() { :; }; set -T; g",
    );
    assert_eq!(code, 0);
    // `declare +ft f` does not clear the function's attribute: as in Bash, `+f` is
    // ignored and the `+t` applies to a variable named `f`.
    assert_eq!(stdout, "in\nret\nret\nret\nret");
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
        root.path(),
        "echo *; GLOBSORT=-name; echo *; GLOBSORT=size; echo *; GLOBSORT=-size; echo *; GLOBSORT=bogus; echo *; touch -t 202001010000 a; touch -t 202101010000 b; touch -t 201901010000 c; GLOBSORT=mtime; echo *; GLOBSORT=-mtime; echo *",
    );
    assert_eq!(code, 0);
    assert_eq!(stdout, "a b c\nc b a\nc a b\nb a c\na b c\nc a b\nb a c");

    let numeric = source_fixture("globsort-numeric");
    for name in ["10", "9", "100", "1"] {
        std::fs::write(numeric.join(name), "").unwrap();
    }
    assert_eq!(
        output_in(
            numeric.path(),
            "echo *; GLOBSORT=numeric; echo *; GLOBSORT=-numeric; echo *"
        ),
        (0, "1 10 100 9\n1 9 10 100\n100 10 9 1".into())
    );
}

#[test]
fn bash_53_bash_source_fullpath_records_the_real_path() {
    let root = source_fixture("source-fullpath");
    std::fs::write(root.join("src.sh"), "echo \"${BASH_SOURCE[0]}\"\n").unwrap();
    let (code, stdout) = output_in(
        root.path(),
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
        output_in(root.path(), ". ./first-line.sh"),
        (0, "ranx\ntwo".into())
    );

    let (code, _, _) = run_script_file(&root.join("does-not-exist.sh"));
    assert_eq!(code, 127);
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

#[test]
fn bash_53_return_trap_sees_the_status_from_before_return() {
    // `$?` in the trap is the status `return` found, not the one it sets; the caller
    // still gets the returned status. Checked against Git Bash 5.3.15 (and Bash 5.2).
    assert_eq!(
        output(concat!(
            "f() { trap 'echo \"in:$?\"' RETURN; false; return 7; }; f; echo \"after:$?\"; ",
            "g() { trap 'echo \"in:$?\"' RETURN; true; return 3; }; g; echo \"after:$?\"; ",
            "h() { trap 'echo \"in:$?\"' RETURN; (exit 4); return; }; h; echo \"after:$?\"; ",
            "k() { trap 'echo \"in:$?\"' RETURN; false; }; k; echo \"after:$?\""
        )),
        (
            0,
            "in:1\nafter:7\nin:0\nafter:3\nin:4\nafter:4\nin:1\nafter:1".into()
        )
    );

    let root = source_fixture("return-status");
    std::fs::write(root.join("rs.sh"), "false; return 6\n").unwrap();
    assert_eq!(
        output_in(
            root.path(),
            "trap 'echo \"in:$?\"' RETURN; . ./rs.sh; echo \"after:$?\""
        ),
        (0, "in:1\nafter:6".into())
    );
}

#[test]
fn a_builtin_expands_an_indexed_subscript_again() {
    // `unset 'a[$i]'` and its kin: the quoted subscript is expanded, then evaluated as
    // arithmetic, as in Bash.
    assert_eq!(
        output(concat!(
            "i='1+1'; ",
            "a=(x y z w); unset 'a[$i]'; echo \"unset ${!a[*]}\"; ",
            "a=(x y z w); read 'a[$i]' <<< S; echo \"read ${a[2]}\"; ",
            "a=(x y z w); printf -v 'a[$i]' R; echo \"printf ${a[2]}\"; ",
            "a=(x y z w); declare 'a[$i]=T'; echo \"declare ${a[2]}\"; ",
            "a=(x y z w); [[ -v 'a[$i]' ]] && echo \"-v set\"; ",
            "a=(x y z w); unset 'a[$((i))]'; echo \"arith ${!a[*]}\""
        )),
        (
            0,
            "unset 0 1 3\nread S\nprintf R\ndeclare T\n-v set\narith 0 1 3".into()
        )
    );
}

#[test]
fn bash_53_array_expand_once_applies_to_indexed_subscripts() {
    // With the option, `unset`, `read` and `printf -v` do not expand the subscript again,
    // so `$i` is an arithmetic error, which abandons the rest of the line as in Bash;
    // `declare` and `[[ -v ]]` still expand it.
    let (stdout, stderr) = script_output(
        "expand-once",
        concat!(
            "shopt -s array_expand_once; i='1+1'\n",
            "a=(x y z w); unset 'a[$i]'; echo same-line\n",
            "echo \"next ${!a[*]}\"\n",
            "printf -v 'a[$i]' R; echo same-line\n",
            "read 'a[$i]' <<< S; echo same-line\n",
            "declare 'a[$i]=T'; echo \"declare ${a[2]}\"\n",
            "[[ -v 'a[$i]' ]] && echo \"-v set\"\n",
            "unset 'a[i]'; echo \"bare name ${!a[*]}\""
        ),
    );
    assert_eq!(stdout, "next 0 1 2 3\ndeclare T\n-v set\nbare name 0 1 3");
    assert_eq!(stderr.lines().count(), 3, "{stderr}");
}

#[test]
fn a_subscript_expanded_again_never_runs_a_command() {
    // cash (spec §4): Bash runs `cmd` here, its best-known array injection. Cash
    // refuses the command substitution with an error and the line goes on.
    let (_, stdout, stderr) = output_with_stderr(concat!(
        "declare -A h=([x]=1); a=(1 2 3); key='$(echo RAN >&2)1'\n",
        "unset \"h[$key]\"; echo \"u=$?\"\n",
        "unset \"a[$key]\"; echo \"i=$? ${!a[*]}\"\n",
        "read \"a[$key]\" <<< v; echo \"r=$?\"\n",
        "printf -v \"a[$key]\" w; echo \"p=$?\"\n",
        "declare \"a[$key]=z\"; echo \"d=$?\"\n",
        "[[ -v \"a[$key]\" ]]; echo \"v=$?\"\n",
        "unset 'a[$(( $(echo RAN >&2) 1))]'; echo \"n=$? ${a[*]}\""
    ));
    assert_eq!(stdout, "u=1\ni=1 0 1 2\nr=1\np=1\nd=1\nv=1\nn=1 1 2 3");
    assert!(!stderr.lines().any(|l| l == "RAN"), "{stderr}");
    assert!(
        stderr.contains("command substitution in a subscript is not run"),
        "{stderr}"
    );
}

#[test]
fn an_arithmetic_error_in_a_builtin_abandons_the_line_but_not_in_let_or_double_parens() {
    let (stdout, _) = script_output(
        "arith-error",
        concat!(
            "a=(x y z)\n",
            "unset 'a[1+]'; echo same-line\n",
            "echo next\n",
            "f() { unset 'a[1+]'; echo in-f; }; f; echo after-f\n",
            "echo next2\n",
            "if unset 'a[1+]'; then echo then; else echo else; fi\n",
            "(( 1+ )); echo \"parens $?\"\n",
            "let '1+'; echo \"let $?\""
        ),
    );
    assert_eq!(stdout, "next\nnext2\nparens 1\nlet 1");

    // Bash runs a `-c` string as one unit: the error abandons the rest of it, and the
    // shell exits 1. An error in an expansion (`$((1+))`) does not.
    assert_eq!(
        output("a=(x y); unset 'a[1+]'\necho next"),
        (1, String::new())
    );
    assert_eq!(output("echo $((1+))\necho next"), (0, "next".into()));
}

/// Runs `script` as a script file, which Bash runs a line at a time.
fn script_output(name: &str, script: &str) -> (String, String) {
    let root = source_fixture(name);
    let file = root.join("script.sh");
    std::fs::write(&file, script).unwrap();
    let result = cash_command()
        .args(["--noprofile", "--norc"])
        .arg(&file)
        .output()
        .unwrap();
    let text = |bytes: Vec<u8>| {
        String::from_utf8(bytes)
            .unwrap()
            .replace("\r\n", "\n")
            .trim_end()
            .to_string()
    };
    (text(result.stdout), text(result.stderr))
}

/// `wait` cases, each in a fresh cash with `$X` naming an external program so that `$!`
/// is a real process. Expected output is Git Bash 5.3.15's.
fn wait_case(script: &str) -> String {
    let cash = CASH.replace('\\', "/");
    let (_, stdout, _) = output_with_stderr(&format!("X='{cash}'\n{script}"));
    stdout
}

#[test]
fn bash_53_wait_n_keeps_the_status_for_wait_pid_except_in_posix_mode() {
    assert_eq!(
        wait_case(
            "\"$X\" -c 'exit 3' & p=$!; sleep 0.5; wait -n; echo \"n=$?\"; wait $p; echo \"p=$?\""
        ),
        "n=3\np=3"
    );
    assert_eq!(
        wait_case(
            "set -o posix; \"$X\" -c 'exit 3' & p=$!; sleep 0.5; wait -n; echo \"n=$?\"; wait $p; echo \"p=$?\""
        ),
        "n=3\np=127"
    );
    // A plain `wait` forgets what `wait -n` returned.
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & p=$!; sleep 0.5; wait -n; wait; wait $p; echo \"p=$?\""),
        "p=127"
    );
    // `wait -n PID`, and `-p`.
    assert_eq!(
        wait_case(concat!(
            "\"$X\" -c 'exit 3' & a=$!; \"$X\" -c 'exit 5' & b=$!; sleep 0.5; ",
            "wait -n -p v $b; echo \"n=$? $([ \"$v\" = \"$b\" ] && echo v)\"; ",
            "wait $b; echo \"b=$?\"; wait $a; echo \"a=$?\""
        )),
        "n=5 v\nb=5\na=3"
    );
}

#[test]
fn exit_in_a_background_job_ends_the_job_not_the_shell_that_waits() {
    // `wait` and `fg` used to exit the waiting shell with the job's status. `fg` needs job
    // control, which a script turns on with `set -m`, as in Bash.
    assert_eq!(
        output(concat!(
            "{ sleep 0.01; exit 3; } & p=$!; wait $p; echo \"wait: $?\"; ",
            "{ exit 4; } & wait %1; echo \"spec: $?\"; ",
            "set -m; { exit 5; } & fg > /dev/null; echo \"fg: $?\""
        )),
        (0, "wait: 3\nspec: 4\nfg: 5".into())
    );
}

#[test]
fn wait_reports_the_status_of_a_job_that_has_already_finished() {
    // Twice, and after the job was reaped when the next one started.
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & p=$!; wait $p; echo \"1=$?\"; wait $p; echo \"2=$?\""),
        "1=3\n2=3"
    );
    assert_eq!(
        wait_case(concat!(
            "\"$X\" -c 'exit 3' & a=$!; sleep 0.5; \"$X\" -c 'exit 4' & b=$!; sleep 0.5; ",
            "wait -n; echo \"n=$?\"; wait -n; echo \"n2=$?\"; wait -n; echo \"n3=$?\"; ",
            "wait $a; echo \"a=$?\""
        )),
        "n=3\nn2=4\nn3=127\na=3"
    );
    // A plain `wait` keeps the status of a job that finished before it.
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & p=$!; sleep 0.5; wait; wait $p; echo \"p=$?\""),
        "p=3"
    );
    // `wait PID` collects the job: `wait -n` does not return it again.
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & p=$!; wait $p; wait -n; echo \"n=$?\""),
        "n=127"
    );
}

/// `wait` after `jobs`, each case in a fresh cash, with `$X` as in [`wait_case`] and
/// `ended`, which returns once the shell has seen every job end: a sleep only makes that
/// likely. `ended` asks `jobs -r` until it lists nothing, in the shell itself, since a
/// pipeline or a command substitution is a subshell with a table of its own. `jobs -r`
/// shows no finished job, so Bash leaves each one in the job table, not yet reported.
/// Expected output is Git Bash 5.3.15's.
fn jobs_case(script: &str) -> String {
    let root = tempfile::tempdir().unwrap();
    let listed = root
        .path()
        .join("running")
        .to_string_lossy()
        .replace('\\', "/");
    let cash = CASH.replace('\\', "/");
    let (_, stdout, _) = output_with_stderr(&format!(
        "X='{cash}'; T='{listed}'\n\
         ended() {{ local n=0; while jobs -r > \"$T\"; [ -s \"$T\" ] && [ $((n += 1)) -lt 200 ]; \
         do sleep 0.05; done; }}\n\
         {script}"
    ));
    stdout
}

#[test]
fn wait_reports_the_status_of_a_finished_job_that_jobs_has_shown() {
    // `jobs` takes a finished job it has shown out of the job table. Its status went
    // with it, and `wait PID` then said the pid was not a child of this shell (127).
    // First as it was reported, where the job ends during a sleep.
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & p=$!; sleep 0.5; jobs > /dev/null; wait $p; echo \"p=$?\""),
        "p=3"
    );
    // A killed job, asked for twice.
    assert_eq!(
        jobs_case(concat!(
            "\"$X\" -c 'sleep 30' & p=$!; kill -KILL $p; ended; jobs > /dev/null; ",
            "wait $p; echo \"p=$?\"; wait $p; echo \"again=$?\""
        )),
        "p=137\nagain=137"
    );
    // It is shown once, and a status of 0 is kept like any other.
    assert_eq!(
        jobs_case("\"$X\" -c 'exit 0' & p=$!; ended; jobs; jobs; wait $p; echo \"p=$?\""),
        "[1]+  Done                       \"$X\" -c 'exit 0'\np=0"
    );
    // A plain `wait` forgets it, as it forgets what `wait -n` returned.
    assert_eq!(
        jobs_case(
            "\"$X\" -c 'exit 3' & p=$!; ended; jobs > /dev/null; wait; wait $p; echo \"p=$?\""
        ),
        "p=127"
    );
    // A job `jobs` has shown has no `%1` any more (`wait: %1: no such job`, 127), as in
    // Bash 5.3: cash kept a finished job's number with its status (TODO 5.1).
    assert_eq!(
        jobs_case("\"$X\" -c 'exit 3' & p=$!; ended; jobs > /dev/null; wait %1; echo \"j=$?\""),
        "j=127"
    );
}

/// Every difference from Bash 5.3 in `open-issues.md` entry 8, each script in a fresh
/// cash, and Git Bash 5.3.15's answer (TODO 5.1).
#[test]
fn jobs_and_wait_answer_as_bash_53_does() {
    // A subshell sees the jobs as they are when it is made: this never ended.
    assert_eq!(
        wait_case(concat!(
            "\"$X\" -c 'exit 3' & sleep 0.5; ",
            "while [ -n \"$(jobs -pr)\" ]; do sleep 0.1; done; echo ended"
        )),
        "ended"
    );
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & sleep 0.5; echo \"$(jobs)\""),
        "[1]+  Exit 3                     \"$X\" -c 'exit 3'"
    );
    // A plain `wait` forgets every status but that of `$!`, when that job ended before
    // it and nothing reported it.
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & p=$!; wait $p; wait; wait $p; echo $?"),
        "127"
    );
    assert_eq!(
        wait_case(concat!(
            "\"$X\" -c 'exit 3' & a=$!; \"$X\" -c 'exit 4' & b=$!; sleep 0.5; ",
            "wait; wait $a; echo $?; wait $b; echo $?"
        )),
        "127\n4"
    );
    assert_eq!(
        wait_case("\"$X\" -c 'sleep 0.3; exit 5' & wait; wait $!; echo $?"),
        "127"
    );
    // A finished job keeps its number until it is reported, and then has none.
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & sleep 0.5; jobs > /dev/null; wait %1; echo $?"),
        "127"
    );
    assert_eq!(
        wait_case("\"$X\" -c 'exit 3' & sleep 0.5; wait -n; wait %1; echo $?"),
        "127"
    );
    assert_eq!(
        wait_case(concat!(
            "\"$X\" -c 'exit 3' & sleep 0.5; \"$X\" -c 'sleep 0.5; exit 4' & ",
            "wait %1; echo $?; wait %2; echo $?"
        )),
        "3\n4"
    );
    assert_eq!(
        wait_case(concat!(
            "for i in 1 2 3; do \"$X\" -c \"exit $i\" & done; sleep 0.5; ",
            "\"$X\" -c 'sleep 1' & jobs"
        )),
        concat!(
            "[1]   Exit 1                     \"$X\" -c \"exit $i\"\n",
            "[2]   Exit 2                     \"$X\" -c \"exit $i\"\n",
            "[3]   Exit 3                     \"$X\" -c \"exit $i\"\n",
            "[4]+  Running                    \"$X\" -c 'sleep 1' &"
        )
    );
    // A job a `wait` collected stays in the table, reported, until the next job or
    // `jobs` takes it out: `%1` finds it again until then, `jobs` does not show it, and
    // `wait -n` does not return it.
    assert_eq!(
        jobs_case(concat!(
            "\"$X\" -c 'exit 5' & ended; wait %1; echo \"j=$?\"; ",
            "wait -n; echo \"n=$?\"; wait %1; echo \"j2=$?\""
        )),
        "j=5\nn=127\nj2=5"
    );
    assert_eq!(
        jobs_case(
            "\"$X\" -c 'exit 3' & ended; wait %1; echo \"j=$?\"; jobs; wait %1; echo \"j2=$?\""
        ),
        "j=3\nj2=127"
    );
    assert_eq!(
        jobs_case(
            "\"$X\" -c 'exit 4' & q=$!; ended; wait $q; echo \"q=$?\"; wait %1; echo \"j=$?\""
        ),
        "q=4\nj=4"
    );
    assert_eq!(
        jobs_case("\"$X\" -c 'exit 7' & ended; wait %1; \"$X\" -c 'exit 8' & ended; jobs"),
        "[1]+  Exit 8                     \"$X\" -c 'exit 8'"
    );
    let (_, stdout, stderr) = output_with_stderr("wait %5; echo $?");
    assert_eq!(stdout.as_str(), "127");
    assert!(
        stderr.ends_with(": line 1: wait: %5: no such job"),
        "{stderr}"
    );
    // `-p` with nothing to wait for unsets the variable.
    assert_eq!(
        wait_case("v=old; wait -n -p v; echo \"${v-unset}\""),
        "unset"
    );
    // POSIX mode shows a failed job as `Done(3)`.
    assert_eq!(
        wait_case("set -o posix; \"$X\" -c 'exit 3' & sleep 0.5; jobs"),
        "[1]+  Done(3)                    \"$X\" -c 'exit 3'"
    );
    // `$!` outlives the job's place in the table, and a subshell has it.
    assert_eq!(
        wait_case(concat!(
            "\"$X\" -c 'exit 3' & p=$!; sleep 0.5; jobs > /dev/null; ",
            "[ \"$!\" = \"$p\" ] && echo kept; (echo \"sub=$!\") | grep -c \"$p\"; ",
            "echo \"$(echo $!)\" | grep -c \"$p\""
        )),
        "kept\n1\n1"
    );
}

/// `${!}` and `$!` with an operator were left as written: the `!` was taken for an
/// indirection (`${!name}`), which needs a name. `${#?}` was `$#` or an error, not the
/// length of `$?`. Before any job `$!` is unset. Git Bash 5.3.15's answers (TODO 5.4).
#[test]
fn braced_special_parameters_expand_as_in_bash() {
    assert_eq!(
        output(r#"echo "[${!}] [${!-none}] [${!:+set}] [${#!}] [${#?}]""#),
        (0, "[] [none] [] [0] [1]".into())
    );
    assert_eq!(
        output(r#"sleep 0.1 & p=$!; [ "${!}" = "$p" ] && echo same; echo "[${!:+set}]""#),
        (0, "same\n[set]".into())
    );
    // `set -u` refuses an unset `$!`, as it refuses `$1`.
    let (code, stdout, stderr) = output_with_stderr(r#"set -u; echo "[$!]"; echo after"#);
    assert_ne!(code, 0);
    assert_eq!(stdout, "");
    assert!(stderr.contains("$!"), "{stderr}");
    // An indirection is still one.
    assert_eq!(
        output(r#"x=y; y=5; echo "${!x} ${!x:-d}""#),
        (0, "5 5".into())
    );
}

#[test]
fn wait_reports_the_status_whichever_way_jobs_showed_the_job() {
    for jobs in ["jobs", "jobs -l", "jobs -n", "jobs %1"] {
        assert_eq!(
            jobs_case(&format!(
                "\"$X\" -c 'exit 3' & p=$!; ended; {jobs} > /dev/null; wait $p; echo \"p=$?\""
            )),
            "p=3",
            "{jobs}"
        );
    }
}

#[test]
fn bash_53_wait_n_passes_over_a_finished_job_that_jobs_has_shown() {
    assert_eq!(
        jobs_case(concat!(
            "\"$X\" -c 'exit 3' & p=$!; ended; jobs > /dev/null; ",
            "wait -n; echo \"n=$?\"; wait $p; echo \"p=$?\""
        )),
        "n=127\np=3"
    );
    // Two finished jobs, one of them shown.
    assert_eq!(
        jobs_case(concat!(
            "\"$X\" -c 'exit 3' & a=$!; \"$X\" -c 'exit 4' & b=$!; ended; jobs %1 > /dev/null; ",
            "wait -n; echo \"n=$?\"; wait -n; echo \"n2=$?\"; ",
            "wait $a; echo \"a=$?\"; wait $b; echo \"b=$?\""
        )),
        "n=4\nn2=127\na=3\nb=4"
    );
    // Asked for by its pid, `wait -n` returns it, each time; and so one that an earlier
    // `wait -n` returned.
    assert_eq!(
        jobs_case(concat!(
            "\"$X\" -c 'exit 3' & p=$!; ended; jobs > /dev/null; wait -n $p; echo \"n=$?\"; ",
            "wait -n -p v $p; echo \"n=$? $([ \"$v\" = \"$p\" ] && echo v)\"; ",
            "wait $p; echo \"p=$?\""
        )),
        "n=3\nn=3 v\np=3"
    );
    assert_eq!(
        jobs_case(
            "\"$X\" -c 'exit 3' & p=$!; ended; wait -n; echo \"n=$?\"; wait -n $p; echo \"n2=$?\""
        ),
        "n=3\nn2=3"
    );
    // In POSIX mode a bare `wait -n` returns it too, and forgets it.
    assert_eq!(
        jobs_case(concat!(
            "set -o posix; \"$X\" -c 'exit 3' & p=$!; ended; jobs > /dev/null; ",
            "wait -n; echo \"n=$?\"; wait -n; echo \"n2=$?\"; wait $p; echo \"p=$?\""
        )),
        "n=3\nn2=127\np=127"
    );
    assert_eq!(
        jobs_case(concat!(
            "set -o posix; \"$X\" -c 'exit 3' & p=$!; ended; jobs > /dev/null; ",
            "wait -n $p; echo \"n=$?\"; wait $p; echo \"p=$?\""
        )),
        "n=3\np=127"
    );
}

#[test]
fn in_posix_mode_wait_forgets_a_status_once_it_has_returned_it() {
    // Whether the job was running, had finished, or had been shown by `jobs`, which
    // leaves nothing for `wait -n` either.
    assert_eq!(
        wait_case(
            "set -o posix; \"$X\" -c 'exit 3' & p=$!; wait $p; echo \"1=$?\"; wait $p; echo \"2=$?\""
        ),
        "1=3\n2=127"
    );
    assert_eq!(
        jobs_case(
            "set -o posix; \"$X\" -c 'exit 3' & p=$!; ended; wait $p; echo \"1=$?\"; wait $p; echo \"2=$?\""
        ),
        "1=3\n2=127"
    );
    assert_eq!(
        jobs_case(concat!(
            "set -o posix; \"$X\" -c 'exit 3' & p=$!; ended; jobs > /dev/null; ",
            "wait $p; echo \"1=$?\"; wait -n; echo \"n=$?\"; wait $p; echo \"2=$?\""
        )),
        "1=3\nn=127\n2=127"
    );
}

#[test]
fn a_finished_job_that_jobs_did_not_show_keeps_its_status() {
    // `jobs` looks at every job and shows the ones it was asked for. A finished job it
    // did not show (`jobs -r`, here in `ended`) stays in the table with no task left to
    // wait for: `wait` reported 0 for it, and the next poll dropped it with its status.
    assert_eq!(
        jobs_case("\"$X\" -c 'exit 3' & p=$!; ended; wait $p; echo \"p=$?\""),
        "p=3"
    );
    assert_eq!(
        jobs_case(
            "\"$X\" -c 'exit 3' & p=$!; ended; wait %1; echo \"j=$?\"; wait %1; echo \"j2=$?\""
        ),
        "j=3\nj2=3"
    );
    assert_eq!(
        jobs_case(concat!(
            "\"$X\" -c 'exit 3' & p=$!; ended; ",
            "wait -n; echo \"n=$?\"; wait -n; echo \"n2=$?\"; wait $p; echo \"p=$?\""
        )),
        "n=3\nn2=127\np=3"
    );
    // A script reaps it when the next job starts, and a plain `wait` when it has no job
    // to wait for.
    assert_eq!(
        jobs_case(concat!(
            "\"$X\" -c 'exit 3' & p=$!; ended; \"$X\" -c 'exit 4' & q=$!; ",
            "wait $p; echo \"p=$?\"; wait $q; echo \"q=$?\""
        )),
        "p=3\nq=4"
    );
    assert_eq!(
        jobs_case("\"$X\" -c 'exit 3' & p=$!; ended; wait; wait $p; echo \"p=$?\""),
        "p=3"
    );
    // `jobs -p` prints a pid and reports nothing: the job is still `wait -n`'s to
    // return, and `jobs -n`'s to show.
    assert_eq!(
        jobs_case("\"$X\" -c 'exit 3' & p=$!; ended; jobs -p > /dev/null; wait -n; echo \"n=$?\""),
        "n=3"
    );
    assert_eq!(
        jobs_case(
            "\"$X\" -c 'exit 3' & p=$!; ended; jobs -p > /dev/null; jobs -n; jobs -n; echo end"
        ),
        "[1]+  Exit 3                     \"$X\" -c 'exit 3'\nend"
    );
}

#[test]
fn a_chld_trap_runs_once_for_each_child_reaped() {
    // cash's CHLD (spec D64): once per process the shell starts and reaps, foreground or
    // in a pipeline, and once per background job, at the points Bash runs a pending
    // trap. Checked against Git Bash 5.3.15 with external programs; `$X` is cash itself,
    // started as one.
    let cash = CASH.replace('\\', "/");
    let script = format!(
        "X='{cash}'\n\
         trap 'echo chld' CHLD\n\
         echo fg; \"$X\" -c true\n\
         echo pipe; \"$X\" -c true | \"$X\" -c true\n\
         echo builtin; true\n\
         echo bg; \"$X\" -c true & \"$X\" -c true & wait\n\
         echo bg-then-fg; \"$X\" -c true & \"$X\" -c 'sleep 1'; echo after\n\
         trap - CHLD; \"$X\" -c true; echo end"
    );
    assert_eq!(
        output(&script),
        (
            0,
            "fg\nchld\npipe\nchld\nchld\nbuiltin\nbg\nchld\nchld\nbg-then-fg\nchld\nchld\nafter\nend"
                .into()
        )
    );
}

#[test]
fn a_chld_trap_that_runs_a_command_does_not_set_itself_off() {
    let cash = CASH.replace('\\', "/");
    let script = format!(
        "X='{cash}'; n=0\n\
         trap 'n=$((n+1)); \"$X\" -c true' CHLD\n\
         \"$X\" -c true; true; true; echo \"runs=$n\""
    );
    assert_eq!(output(&script), (0, "runs=1".into()));
}

#[test]
fn a_signal_the_shell_sends_itself_runs_its_trap_or_its_default() {
    // `kill -SIG $$` is handled inside the shell, as in Bash: delivered through Windows it
    // ended cash at once, trap or no trap. Checked against Git Bash 5.3.15.
    assert_eq!(
        output(concat!(
            "trap 'echo \"sig=$BASH_TRAPSIG\"' INT TERM HUP\n",
            "kill -INT $$; kill -TERM $$; kill -HUP $$; echo alive"
        )),
        (0, "sig=2\nsig=15\nsig=1\nalive".into())
    );
    // Untrapped, a script ends with 128 + the signal, except QUIT, which Bash ignores.
    for (signal, status) in [("TERM", 143), ("INT", 130), ("HUP", 129), ("KILL", 137)] {
        assert_eq!(
            output(&format!("kill -{signal} $$; echo survived")),
            (status, String::new()),
            "{signal}"
        );
    }
    assert_eq!(
        output("kill -QUIT $$; echo survived"),
        (0, "survived".into())
    );
}

#[test]
fn kill_defaults_to_term_and_a_killed_process_reports_bashs_status() {
    // `kill $pid` sends TERM, as in Bash (it sent KILL), and a process a signal ends
    // exits with 128 + the signal's number, which `wait` reports: 143, 137, 130.
    let cash = CASH.replace('\\', "/");
    let script = format!(
        "X='{cash}'\n\
         \"$X\" -c 'sleep 5' & p=$!; kill $p; wait $p; echo \"default=$?\"\n\
         \"$X\" -c 'sleep 5' & p=$!; kill -KILL $p; wait $p; echo \"kill=$?\"\n\
         \"$X\" -c 'sleep 5' & p=$!; kill -INT $p; wait $p; echo \"int=$?\""
    );
    assert_eq!(
        output(&script),
        (0, "default=143\nkill=137\nint=130".into())
    );
}

fn output_with_args(script: &str, args: &[&str]) -> String {
    let result = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .args(args)
        .output()
        .unwrap();
    String::from_utf8(result.stdout)
        .unwrap()
        .replace("\r\n", "\n")
}

#[test]
fn bash_argv_is_bashs_stack_seeded_once() {
    // pure-bash-bible's reverse_array: the first `shopt -s extdebug` puts the function's
    // own arguments on the stack, and they stay there after it returns (Git Bash 5.3.15).
    let reverse = "reverse_array() { shopt -s extdebug; f()(printf '%s ' \"${BASH_ARGV[@]}\"); f \"$@\"; shopt -u extdebug; echo; }\n\
                   reverse_array 1 2 3\n\
                   reverse_array red blue";
    assert_eq!(
        output_with_args(reverse, &[]),
        "3 2 1 3 2 1 \nblue red 3 2 1 \n"
    );

    // Read first inside a function, it is empty; read at the top, it takes `$@` as it is
    // then; a function's entry is its arguments when called, whatever `set --` does.
    let script = "f() { echo \"f [${BASH_ARGV[*]}]\"; }; f 1 2\n\
                  set -- Z; echo \"top [${BASH_ARGV[*]}] [${BASH_ARGC[*]}]\"\n\
                  shopt -s extdebug\n\
                  g() { set -- w; echo \"g [${BASH_ARGV[*]}] [${BASH_ARGC[*]}]\"; }; g 3 4\n\
                  shopt -u extdebug; h() { echo \"h [${BASH_ARGV[*]}]\"; }; h 5";
    assert_eq!(
        output_with_args(script, &["name", "A", "B"]),
        "f []\ntop [Z] [1]\ng [4 3 Z] [2 1]\nh [Z]\n"
    );
}

#[test]
fn prompt_expansion_keeps_quote_characters() {
    // Bash expands a decoded prompt as if inside double quotes (Q_DOUBLE_QUOTES), so `"`
    // and `'` stay literal; only `\$ \` \" \\` and `\<newline>` drop their backslash.
    // Expected output is Git Bash 5.3.15's.
    let script = r#"PS1='a"b"c \[x\] '\''d'\'' e'; printf '<%s>\n' "${PS1@P}"
v=hi; p='q\"r \\ s\x "$v" '\''$v'\'' ${v:-"z"} $(echo "c") \$'; printf '<%s>\n' "${p@P}"
p="it's"; printf '<%s>\n' "${p@P}"
p='\\\" \\\\'; printf '<%s>\n' "${p@P}"
p='a\
b `echo "x y"` \`'; printf '<%s>\n' "${p@P}""#;
    // `\$` is bash's `#` when EUID is 0, which an elevated process is, as on GitHub's
    // runners, and `$` otherwise.
    let prompt_sign = if output("echo $EUID").1 == "0" {
        '#'
    } else {
        '$'
    };
    assert_eq!(
        output(script),
        (
            0,
            format!(
                r#"<a"b"c x 'd' e>
<q"r \ s\x "hi" 'hi' hi c {prompt_sign}>
<it's>
<\" \>
<ab x y `>"#
            )
        )
    );
}

#[test]
fn pipestatus_is_left_to_the_commands_a_compound_command_ran() {
    // A compound command that is the whole pipeline leaves PIPESTATUS as the commands
    // inside set it, and so does a function's definition; a subshell, `[[ ]]`, `(( ))`
    // and a function's call set it as a simple command does. Expected output is Git Bash
    // 5.3.15's.
    let script = r#"if true; then false | true; fi; echo "if: ${PIPESTATUS[*]}"
if false; then :; else false | true; fi; echo "else: ${PIPESTATUS[*]}"
{ false | true; } > /dev/null; echo "group: ${PIPESTATUS[*]}"
! { false | true; }; echo "negated group: ${PIPESTATUS[*]}"
( false | true ); echo "subshell: ${PIPESTATUS[*]}"
while false | true; do break; done; echo "while: ${PIPESTATUS[*]}"
for x in 1; do false | true; done; echo "for: ${PIPESTATUS[*]}"
false | true; for x in; do :; done; echo "empty for: ${PIPESTATUS[*]}"
for ((i=0;i<1;i++)); do false | true; done; echo "arith for: ${PIPESTATUS[*]}"
false | true; case y in x) :;; esac; echo "case: ${PIPESTATUS[*]}"
f() { false | true; }; f; echo "function: ${PIPESTATUS[*]}"
false | true; g() { :; }; echo "define: ${PIPESTATUS[*]}"
false | true; if false; then :; fi; echo "if false: ${PIPESTATUS[*]}"
if true; then false | true; fi | cat; echo "piped if: ${PIPESTATUS[*]}"
false | true; [[ 1 ]]; echo "[[: ${PIPESTATUS[*]}"
false | true; (( 1 )); echo "((: ${PIPESTATUS[*]}""#;
    assert_eq!(
        output(script),
        (
            0,
            [
                "if: 1 0",
                "else: 1 0",
                "group: 1 0",
                "negated group: 1 0",
                "subshell: 0",
                "while: 0",
                "for: 1 0",
                "empty for: 1 0",
                "arith for: 1 0",
                "case: 1 0",
                "function: 0",
                "define: 1 0",
                "if false: 1",
                "piped if: 0 0",
                "[[: 0",
                "((: 0",
            ]
            .join("\n")
        )
    );
}

/// `-x` on a directory, `-O` and `-G` on the user's own file, and `-N`, as Bash answers
/// them: `-x .` was false, `-O` and `-G` compared a uid of 0 with the account's, and
/// `-N` was "not yet implemented". A true `-N` is not asked: Defender reads a new file
/// and moves its access time when it likes.
#[test]
fn test_reads_directories_owners_and_reads_as_bash_does() {
    let scratch = Scratch::new("test-predicates");
    let script = r#"echo hi > own.txt; mkdir sub
for op in -x -O -G; do
  for f in own.txt sub nosuch; do
    if [ $op "$f" ]; then a=yes; else a=no; fi
    if eval "[[ $op \"$f\" ]]"; then b=yes; else b=no; fi
    echo "$op $f $a $b"
  done
done
[ -N nosuch ]; echo "nosuch -N $?"
touch -d 2020-01-01 old.txt; [ -N old.txt ]; echo "old -N $?""#;
    assert_eq!(
        output_in(scratch.path(), script),
        (
            0,
            [
                "-x own.txt no no",
                "-x sub yes yes",
                "-x nosuch no no",
                "-O own.txt yes yes",
                "-O sub yes yes",
                "-O nosuch no no",
                "-G own.txt yes yes",
                "-G sub yes yes",
                "-G nosuch no no",
                "nosuch -N 1",
                "old -N 1",
            ]
            .join("\n")
        )
    );
}

/// `export` as Bash has it, found writing its man page: a name not yet set is made
/// exported, so a later value is passed on; `-p` quotes a value to be read back and
/// writes an array whole; `-f` and `-pf` list the exported functions; a word that is
/// no name is refused with status 1; an array is never passed to a program.
#[test]
fn export_marks_unset_names_quotes_its_listing_and_refuses_bad_names() {
    let script = r#"export U; U=5; printenv U
export -p | grep ' U='
Q='a"b$c`d\e'; export Q; export -p | grep ' Q='
arr=(x y); export arr; export -p | grep ' arr='
printenv arr || echo 'arr not passed'
greet() { echo hi; }; export -f greet; export -pf
export 1bad=x ok=1 'sp ace' 'a[1]=2'; echo "status $?"; printenv ok
export -f nosuchfn; echo "no function $?"
set -o posix; export -p | grep ' ok='"#;
    let result = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert_eq!(
        stdout.replace("\r\n", "\n").trim_end(),
        [
            "5",
            "declare -x U=\"5\"",
            r#"declare -x Q="a\"b\$c\`d\\e""#,
            r#"declare -ax arr=([0]="x" [1]="y")"#,
            "arr not passed",
            "greet () ",
            "{ ",
            "    echo hi",
            "}",
            "declare -fx greet",
            "status 1",
            "1",
            "no function 1",
            "export ok=\"1\"",
        ]
        .join("\n")
    );
    for refused in ["`1bad=x'", "`sp ace'", "`a[1]'"] {
        assert!(
            stderr.contains(&format!("export: {refused}: not a valid identifier")),
            "{refused}: {stderr}"
        );
    }
}

/// `read -t 0` only asks whether input waits: on a closed standard input it is status 1
/// with no message, as in Bash; cash said "read error: Bad file descriptor".
#[test]
fn read_t_0_on_closed_input_is_quiet() {
    let result = cash_command()
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "read -t 0 <&-; echo \"status $?\"",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim_end(),
        "status 1"
    );
    assert_eq!(String::from_utf8(result.stderr).unwrap(), "");
}

/// `alias` refuses a name no command word could be, and lists a name starting with `-`
/// after `--`, as Bash does; cash took `alias 'a b=1'` and listed `alias -x=...`.
#[test]
fn alias_refuses_bad_names_and_lists_dashed_ones_after_two_dashes() {
    let script = r"alias 'a b=1' ok='echo ok' 'x/y=2'; echo status $?
alias -- -x='echo dash'; alias -- -x; alias ok";
    let result = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    let stderr = String::from_utf8(result.stderr).unwrap();
    assert_eq!(
        stdout.replace("\r\n", "\n").trim_end(),
        "status 1\nalias -- -x='echo dash'\nalias ok='echo ok'"
    );
    assert!(
        stderr.contains("alias: `a b': invalid alias name")
            && stderr.contains("alias: `x/y': invalid alias name"),
        "{stderr}"
    );
}

/// In POSIX mode `alias` lists `name='value'` without `alias `, unless `-p` asks for the
/// form that reads back; `command -v` keeps it. As Bash does; modernish reads the bare
/// form, and took cash's for an alias named `alias`.
#[test]
fn alias_lists_bare_definitions_in_posix_mode() {
    assert_eq!(
        output("set -o posix; alias a=b; alias -- -x=y; alias; alias a; alias -p; command -v a"),
        (
            0,
            "-x='y'\na='b'\na='b'\nalias -- -x='y'\nalias a='b'\nalias a='b'".into()
        )
    );
}

/// What `command` and `exec` say of a program that is not there, and `pwd` with an
/// operand, as Bash: `command x` names `x` alone, not `command: x: ...`; `exec ./x`
/// says the file is not there; `pwd extra` ignores the operand. Found writing alias's
/// and pwd's man pages.
#[test]
fn command_exec_and_pwd_speak_as_bash_does() {
    let script = r#"command nosuchcmd; echo "command $?"
(exec ./nosuch); echo "exec $?"
pwd extra >/dev/null; echo "pwd $?""#;
    let result = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    let stderr = String::from_utf8(result.stderr)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(
        stdout.replace("\r\n", "\n").trim_end(),
        "command 127\nexec 127\npwd 0"
    );
    let messages: Vec<&str> = stderr
        .lines()
        .map(|line| line.split_once(": line ").map_or(line, |(_, rest)| rest))
        .collect();
    assert_eq!(
        messages,
        [
            "1: nosuchcmd: command not found",
            "2: ./nosuch: No such file or directory"
        ],
        "{stderr}"
    );
}

/// An alias is read as text, as Bash reads it: a quote its text opens is closed by the
/// line, the blank after its word stays, and an alias at the end of the input still
/// expands. cash expanded aliases on the tokens after the line was read, so `q quoted'`
/// was an unterminated quote. Each line is Git Bash 5.3's.
#[test]
fn an_alias_is_read_as_text_into_the_line() {
    let script = "shopt -s expand_aliases
alias q=\"echo it's\"
alias greet='echo hello,'
alias e='echo '
alias x='echo chained'
alias tabbed='echo a\t'
q quoted'
q joined' 'here'
greet world
greet 2>/dev/null; echo after
e x
tabbed b
cat <<EOF; x
body
EOF
x";
    assert_eq!(
        output(script),
        (
            0,
            [
                "its quoted",
                "its joined here",
                "hello, world",
                // `2>` sends standard error away, not the greeting.
                "hello,",
                "after",
                // `e`'s text ends in a blank, so `x` is an alias too: `echo echo chained`.
                "echo chained",
                "a b",
                "body",
                "chained",
                "chained",
            ]
            .join("\n")
        )
    );
}

#[test]
fn a_script_run_is_main_in_funcname_caller_and_bash_source() {
    // FUNCNAME left the script out, and caller named its frame `source`, as a sourced
    // file is named; Bash names it `main`, from line 0.
    let root = Scratch::new("funcname-main");
    std::fs::write(
        root.path().join("main.sh"),
        "f() { echo \"${FUNCNAME[*]} ${BASH_SOURCE[*]} ${BASH_LINENO[*]}\"; caller 0; }\nf\n",
    )
    .unwrap();
    assert_eq!(
        output_in(root.path(), "bash main.sh"),
        (0, "f main main.sh main.sh 2 0\n2 main main.sh".into())
    );
}

#[test]
fn a_braced_expansion_or_substitution_on_a_here_document_line_keeps_its_words() {
    // With a here-document pending on the line, each word inside a `${...}` or `$(...)`
    // was held for after its body: `cat <<EOF >"${f}"` read `${}` (shunit2), and a
    // here-document inside `$(...)` ran out of input.
    let script = "f=/dev/stdout x=X\ncat <<EOF >\"${f}\"\na ${x}\nEOF\n\
        cat <<EOF; echo \"$(cat <<IN\nb\nIN\n)\"\nc\nEOF\n\
        echo \"${y:-$(cat <<EOF)}\"\nd\nEOF\n";
    assert_eq!(output(script), (0, "a X\nc\nb\nd".into()));
}

#[test]
fn a_trap_running_is_running_in_its_command_substitutions() {
    // A `$(...)` in a DEBUG trap's handler ran the trap again with functrace on, and again
    // in that one's, to the limit of 500 calls: bats normalises a `C:/` path so in its
    // handler. Bash runs none there; a `( ... )` subshell starts with none running.
    let (status, out) = output(
        "set -T; n=0; trap 'n=$((n + 1)); x=$(echo sub)' DEBUG; : one; : two; trap - DEBUG; \
         echo \"debug ran $n times\"; trap 'echo parent; (trap \"echo child\" EXIT; true)' EXIT",
    );
    assert_eq!(
        (status, out.as_str()),
        (0, "debug ran 3 times\nparent\nchild")
    );
}

#[test]
fn an_empty_arrays_alternative_in_quotes_is_no_word() {
    // `"${a[@]+word}"` gives no word when the array is empty, as `"${a[@]}"` gives none:
    // bats' `BATS_TEST_TAGS=("${tags[@]+${tags[@]}}")` held one empty tag.
    let (status, out) = output(
        "unset t; a=(\"${t[@]+${t[@]}}\"); b=(\"${t[@]:+y}\"); set --; c=(\"${@+$@}\"); \
         d=(\"${t[@]-}\"); e=(\"${t[*]+z}\"); u=(x y); f=(\"${u[@]+${u[@]}}\"); \
         echo ${#a[@]} ${#b[@]} ${#c[@]} ${#d[@]} ${#e[@]} ${#f[@]}",
    );
    assert_eq!((status, out.as_str()), (0, "0 0 0 1 1 2"));
}

#[test]
fn set_e_lets_a_compound_command_pass_on_a_failure_it_ignored() {
    // A loop, `if`, `case` or `{ }` whose status is a failure `set -e` did not act on inside
    // it (the left of `&&`, a `!` pipeline) does not end the shell, as in Bash; a function,
    // `source` and `( )` still do. bashunit's `for ...; do [ -n "$v" ] && ...; done` ended
    // its script.
    let (status, out) = output(
        "set -e; for i in 1; do false && true; done; echo for; { false && true; }; echo group; \
         if true; then ! true; fi; echo if; case a in a) false && true;; esac; echo case; \
         ( false && true ); echo never",
    );
    assert_eq!((status, out.as_str()), (1, "for\ngroup\nif\ncase"));
    let (status, out) = output("set -e; f() { false && true; }; f; echo never");
    assert_eq!((status, out.as_str()), (1, ""));
}

#[test]
fn declare_f_with_extdebug_gives_the_line_and_file() {
    // bashunit keeps the tests a file defines by `declare -F` under extdebug, which gives
    // `NAME LINE FILE` in Bash; cash gave the name alone, and bashunit found no tests.
    let root = Scratch::new("declare-f-extdebug");
    std::fs::write(root.path().join("defs.sh"), "f() { :; }\n\ng() {\n:\n}\n").unwrap();
    assert_eq!(
        output_in(
            root.path(),
            "source defs.sh; shopt -s extdebug; declare -F f g; shopt -u extdebug; declare -F f"
        ),
        (0, "f 1 defs.sh\ng 3 defs.sh\nf".into())
    );
}

#[test]
fn a_function_name_may_end_in_an_equals_sign() {
    // bash-oo-framework defines `boolean.=() {`: a word that ends in `=` is a name unless
    // it starts an assignment, `x=` of `x=()`; after `function` any word is.
    let (status, out) = output(
        "boolean.=() { echo eq; }; boolean.=; function h=() { echo h; }; type -t h=; \
         x=(); echo \"x: ${#x[@]}\"",
    );
    assert_eq!((status, out.as_str()), (0, "eq\nfunction\nx: 0"));
}

#[test]
fn an_alias_after_a_closing_word_is_expanded() {
    // Bash looks for an alias after `}`, `fi`, `done` and `esac`: bash-oo-framework's
    // `try { ...; } catch { ...; }`, whose `catch` closes the subshell `try` opened.
    let (status, out) = output(
        "shopt -s expand_aliases; alias try='( '; alias catch=') || '\n\
         try { echo body; false; } catch { echo caught; }\n\
         try { echo fine; } catch { echo not; }",
    );
    assert_eq!((status, out.as_str()), (0, "body\ncaught\nfine"));
}

#[test]
fn exit_does_not_run_the_err_trap() {
    // `exit` leaves before Bash checks for ERR, as `return` does: bash-oo-framework's
    // ERR handler threw an exception for its `exit 1`, in a function too with errtrace.
    let (status, out) = output(
        "trap 'echo ERR $?' ERR; (exit 4); f() { exit 3; }; set -E; (f); echo \"status $?\"; exit 2",
    );
    assert_eq!((status, out.as_str()), (2, "ERR 4\nERR 3\nstatus 3"));
}

#[test]
fn declare_a_with_a_subscript_takes_a_list_for_the_whole_array() {
    // With `-a` or `-A`, a list given to `name[...]` is the whole array's, the subscript
    // ignored, `@` too: bash3boilerplate appends with `declare -a "arg_i[@]"='("${!v}" "$x")'`.
    // Without them, it is the element's text.
    let (status, out) = output(
        r#"a=(p q); v="a[@]"; declare -a "a[@]"='("${!v}" "z")'; declare -p a
           declare -a "b[0]"="(x y)"; declare -A "m[k]"="([a]=1)"; declare -p b m
           f() { local -a "c[*]"="(u v)"; declare -p c; }; f
           d=(1); declare "d[0]"="(x y)"; declare -p d"#,
    );
    assert_eq!(
        (status, out.as_str()),
        (
            0,
            "declare -a a=([0]=\"p\" [1]=\"q\" [2]=\"z\")\ndeclare -a b=([0]=\"x\" [1]=\"y\")\n\
             declare -A m=([a]=\"1\" )\ndeclare -a c=([0]=\"u\" [1]=\"v\")\ndeclare -a d=([0]=\"(x y)\")"
        )
    );
}

#[test]
fn bash_source_holds_the_sourcing_files_outside_a_function_and_in_the_exit_trap() {
    // A file sourced by the script sees `(file script)` in BASH_SOURCE, which
    // bash3boilerplate reads as `${BASH_SOURCE[1]}`; cash gave the file alone. And the
    // script's EXIT trap runs while it is still the script, with its `$@`.
    let root = Scratch::new("bash-source-chain");
    std::fs::write(root.join("lib.sh"), "echo \"lib: ${BASH_SOURCE[*]}\"\n").unwrap();
    std::fs::write(
        root.join("main.sh"),
        "trap 'echo \"trap: [$*] [${BASH_SOURCE[*]}]\"' EXIT\nsource ./lib.sh\nset -- changed one\n",
    )
    .unwrap();
    let out = cash_command()
        .current_dir(root.path())
        .args(["--noprofile", "--norc", "main.sh", "x"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        "lib: ./lib.sh main.sh\ntrap: [changed one] [main.sh]\n"
    );
}

#[test]
fn the_exit_trap_runs_where_exit_or_errexit_ends_the_shell() {
    // As in Bash: inside a function, `FUNCNAME` in the trap holds it (`f main` in a script),
    // for `exit` and `set -e` alike; it runs once; and an `exit` in it with no status keeps
    // the status the shell was exiting with.
    let trap = "trap 'echo \"trap [$?] [${FUNCNAME[*]-}]\"' EXIT";
    assert_eq!(
        output(&format!("{trap}; f() {{ exit 7; }}; f; echo no")),
        (7, "trap [7] [f]".into())
    );
    assert_eq!(
        output(&format!("set -e; {trap}; h() {{ false; }}; h")),
        (1, "trap [1] [h]".into())
    );
    assert_eq!(
        output("trap 'echo in trap; exit' EXIT; exit 9"),
        (9, "in trap".into())
    );
    assert_eq!(
        output("trap 'echo outer' EXIT; (trap 'echo sub' EXIT; exit 3); echo \"sub $?\"; exit 1"),
        (1, "sub\nsub 3\nouter".into())
    );
}

#[test]
fn err_trap_names_a_pipelines_last_line_once() {
    // A pipeline over several lines fails on the line of its last stage, where Bash has
    // `$LINENO`; the `if` around it, whose status is the pipeline's, runs no trap of its
    // own: the trap ran twice. Then the function's call fails, where it was made.
    let script = "set -E\ntrap 'echo \"ERR $LINENO\"' ERR\nf() {\n  if true; then\n    echo a \\\n      | cat \\\n      | false\n  fi\n}\nf";
    assert_eq!(output(script), (1, "ERR 7\nERR 10".into()));
}

#[test]
fn bash_lineno_has_the_scripts_entry_outside_functions() {
    // A script run, outside any function, has `BASH_LINENO=(0)`, as in Bash, whose bats
    // reads it for its traces; a file it sources adds the line it was sourced from.
    let root = Scratch::new("bash-lineno");
    std::fs::write(
        root.join("sourced.sh"),
        "echo \"sourced [${BASH_LINENO[*]}]\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("main.sh"),
        "echo \"main [${BASH_LINENO[*]}] ${#BASH_LINENO[@]}\"\n. ./sourced.sh\n",
    )
    .unwrap();
    let result = cash_command()
        .current_dir(root.path())
        .args(["--noprofile", "--norc", "main.sh"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).replace("\r\n", "\n"),
        "main [0] 1\nsourced [2 0]\n"
    );
    assert_eq!(
        output("echo \"[${BASH_LINENO[*]}] ${#BASH_LINENO[@]}\""),
        (0, "[] 0".into())
    );
}

#[test]
fn a_sourced_file_runs_without_the_debug_trap_unless_functrace() {
    // As Bash's `source` does (bashdb lives on it): the trap is set aside while the file
    // runs and put back after, unless the file set one of its own, which stays; with
    // `set -T` the file runs under it.
    let root = Scratch::new("sourced-debug");
    std::fs::write(root.join("plain.sh"), "echo \"in file\"\ntrap -p DEBUG\n").unwrap();
    std::fs::write(root.join("sets.sh"), "trap 'echo own' DEBUG\n").unwrap();
    let script = "trap 'echo \"debug: $BASH_COMMAND\"' DEBUG\nsource plain.sh\ntrap - DEBUG\n\
                  trap ': outer' DEBUG\nsource sets.sh\ntrap -p DEBUG | sed 's/^/after sets: /'\n\
                  trap - DEBUG\nset -T\ntrap ': traced' DEBUG\n\
                  source plain.sh | sed 's/^/functrace: /'\ntrap - DEBUG\n";
    assert_eq!(
        output_in(root.path(), script),
        (
            0,
            "debug: source plain.sh\nin file\ndebug: trap - DEBUG\nown\nown\n\
             after sets: trap -- 'echo own' DEBUG\nown\nfunctrace: in file\n\
             functrace: trap -- ': traced' DEBUG"
                .into()
        )
    );
}

#[test]
fn bash_argv_holds_the_arguments_of_a_file_sourced_at_the_top() {
    // With extdebug, a `source` at the top pushes its arguments as a function call does;
    // one in a function adds them to the function's.
    let root = Scratch::new("source-argv");
    std::fs::write(
        root.join("args.sh"),
        "echo \"argv=[${BASH_ARGV[*]}] argc=[${BASH_ARGC[*]}]\"\n",
    )
    .unwrap();
    assert_eq!(
        output_in(
            root.path(),
            "shopt -s extdebug; source args.sh one two; f() { source args.sh three; }; f x y"
        ),
        (
            0,
            "argv=[two one] argc=[2 0]\nargv=[three y x] argc=[1 2 0]".into()
        )
    );
}

#[test]
fn a_subscript_may_hold_spaces_where_an_assignment_may_stand() {
    // `m[k one]=v` and `([k one]=v)` are one assignment word each, as in Bash (composure's
    // metadata); the space ended the word, and `one]=v` was a command.
    assert_eq!(
        output(
            "declare -A m; m[k one]=v1; m[two words here]=v2; declare -A n=([k one]=a [k two]=b)\n\
             echo \"${m[k one]} ${m[two words here]} ${n[k one]} ${n[k two]} ${#n[@]}\"\n\
             i=3; a[i + 1]=four; echo \"${a[4]}\"; echo [a b]"
        ),
        (0, "v1 v2 a b 2\nfour\n[a b]".into())
    );
}

#[test]
fn a_process_substitution_is_an_operand_of_double_brackets() {
    // `[[ -p <(cmd) ]]` was a syntax error; its path is a pipe while it is open, which
    // `-p`, `-e` and `-r` see, and no regular file or directory (bash_unit tests so).
    assert_eq!(
        output(
            "[[ -p <(true) ]] && echo p; [[ -e <( echo x ) ]] && echo e; \
             [[ -r <(true) ]] && echo r; [[ -f <(true) ]] || echo not-f; \
             [[ -d <(true) ]] || echo not-d; [[ $(cat <(echo hi)) == hi ]] && echo read; \
             [ -p <(true) ] && echo single"
        ),
        (0, "p\ne\nr\nnot-f\nnot-d\nread\nsingle".into())
    );
}

#[test]
fn declare_f_prints_here_documents_that_read_back() {
    // A here-document's body follows the line its command ends, after a `;` or `|` that
    // goes on, as Bash prints it; it was printed where the redirection stood, and the
    // function read back ran its body as commands (composure's `write`).
    let function = "g() {\n  echo a\n  cat <<END\nbody\nEND\n  cat <<A; echo c\nx\nA\n  \
                    cat <<B | tr a b\naaa\nB\n}\n";
    assert_eq!(
        output(&format!("{function}eval \"$(declare -f g)\" && g")),
        (0, "a\nbody\nx\nc\nbbb".into())
    );
}

#[test]
fn declaration_options_end_at_the_first_name_or_a_double_dash() {
    // As Bash's getopt takes them: `typeset -f -- f` names `f`, and a `-i` after a name is
    // a name, refused, not an option for what came before.
    assert_eq!(
        output("h() { echo h; }; typeset -f -- h"),
        (0, "h () \n{ \n    echo h\n}".into())
    );
    assert_eq!(
        output("declare x -i 2>/dev/null; echo \"status $?\"; x=1+1; echo \"$x\""),
        (0, "status 1\n1+1".into())
    );
}

#[test]
fn a_windows_name_that_is_no_identifier_is_passed_on_but_no_variable() {
    // `CommonProgramFiles(x86)` is no name a shell can read back: Bash keeps such names
    // out of its variables and passes them to programs as they came. cash listed it in
    // `export -p`, and modernish, which evals that listing, died of a syntax error.
    let result = cash_command()
        .env("ODD(x86)", "1")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "export -p | grep -c 'ODD(x86)'; set | grep -c 'ODD(x86)'; \
             declare -p | grep -c 'ODD(x86)'; cmd.exe /d /c set ODD",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout)
        .unwrap()
        .replace("\r\n", "\n");
    assert_eq!(stdout.trim_end(), "0\n0\n0\nODD(x86)=1");
}

#[test]
fn set_a_exports_functions_and_a_function_defined_again_keeps_its_attributes() {
    // Bash's `set -a` exports "each variable or function that is created or modified";
    // bats runs every test as a process of its own, and the libraries bash-it loads under
    // `set -a` were missing in each. A function defined again keeps `export -f` and
    // `declare -ft`, as in Bash; it lost them.
    assert_eq!(
        output(
            "set -a; f() { echo f; }; set +a; \"$BASH\" -c f; \
             g() { echo 1; }; export -f g; declare -ft g; g() { echo 2; }; \
             \"$BASH\" -c g; declare -F -p g"
        ),
        (0, "f\n2\ndeclare -ftx g".into())
    );
}

#[test]
fn set_a_exports_what_every_kind_of_assignment_sets_but_no_array() {
    // Only `NAME=value` and `declare` exported under `set -a`: the environment's own
    // switch for it was never set. bash-it's log library sets its levels with `${x:=n}`
    // under `set -a`, and every bats test, a process of its own, found them unset.
    assert_eq!(
        output(
            "unset OLDPWD; set -a; : \"${A:=6}\"; read -r B <<< 7; printf -v C 8; for D in 9; do :; done; \
             (( E = 10 )); let F=11; getopts a J -a; cd ..; x=(1 2); set +a; K=12; \
             \"$BASH\" -c 'echo A=$A B=$B C=$C D=$D E=$E F=$F J=$J K=$K; \
             [[ $OLDPWD ]] && echo OLDPWD'; declare -p x"
        ),
        (
            0,
            "A=6 B=7 C=8 D=9 E=10 F=11 J=a K=\nOLDPWD\ndeclare -a x=([0]=\"1\" [1]=\"2\")".into()
        )
    );
}

#[test]
fn the_parser_takes_what_bash_takes_of_time_case_and_here_documents() {
    // `! time` ran a program named `time` and `time --` one named `--`; the `esac` of a
    // pattern ended a `case` inside `$(...)`; a here-document there ended by `EOF)` was
    // read to the end of the input; and `<<-'<TAB>END'` never ended.
    let script = "! time true 2>/dev/null; echo \"bang $?\"\n\
                  TIMEFORMAT=; time time true; time -- true 2>/dev/null; echo \"time $?\"\n\
                  echo $(case x in in|esac) echo no;; x) echo yes;; esac)\n\
                  echo $(case a in a) for i in 1 2; do echo $i; done;; esac)\n\
                  v=$(cat <<EOF\nhi\nEOF)\necho \"[$v]\"\n\
                  cat <<-'\tEND'\n\tbody\n\tEND\necho after";
    assert_eq!(
        output(script),
        (0, "bang 1\ntime 0\nyes\n1 2\n[hi]\nbody\nafter".into())
    );
}

#[test]
fn a_script_s_history_is_written_as_it_ends_to_the_last_histfile() {
    // It was appended line by line to every HISTFILE the script set; Bash writes it once,
    // at the end, to the HISTFILE of then. And `fc -0` and `fc -s -0` ran the last
    // command again, where Bash names none.
    let scratch = Scratch::new("script-history");
    std::fs::write(scratch.path().join("a.list"), "old entry\n").unwrap();
    std::fs::write(
        scratch.path().join("h.sh"),
        "HISTFILE=$PWD/a.list\nset -o history\necho first >/dev/null\n\
         HISTFILE=$PWD/b.list\necho second >/dev/null\n[ -e b.list ] || echo none yet\n\
         fc -0 2>/dev/null; echo \"fc $?\"; fc -s -0 2>/dev/null; echo \"fcs $?\"\n",
    )
    .unwrap();
    let result = cash_command()
        .current_dir(scratch.path())
        .env("FCEDIT", "true")
        .args(["--noprofile", "--norc", "h.sh"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(result.stdout)
            .unwrap()
            .replace("\r\n", "\n"),
        "none yet\nfc 1\nfcs 1\n"
    );
    let read = |name: &str| {
        std::fs::read_to_string(scratch.path().join(name))
            .unwrap()
            .replace("\r\n", "\n")
    };
    assert_eq!(read("a.list"), "old entry\n");
    let written = read("b.list");
    assert!(
        written.starts_with("echo first >/dev/null\nHISTFILE=$PWD/b.list\n"),
        "{written}"
    );
}

#[test]
fn getopts_reads_h_and_help_after_its_name_as_the_script_s_options() {
    // A script given `-h` got cash's help for getopts and no option: clap's help flag
    // took the word anywhere. Only a first `--help` is getopts' own, as in Bash.
    assert_eq!(
        output(
            "set -- -h -v; while getopts hv opt \"$@\"; do echo \"opt=$opt\"; done; \
             getopts --help | head -1"
        ),
        (0, "opt=h\nopt=v\nParse command options".into())
    );
}

#[test]
fn a_leading_tilde_or_hash_is_quoted_so_a_listing_reads_back() {
    // `set` listed `V=~`, which reads back as the home directory, and `#x` as a comment;
    // Bash quotes a `~` that leads or follows `=` or `:`, and a leading `#`, and `declare
    // -p` quotes a key `@`, which unquoted means every element.
    assert_eq!(
        output(
            "V='~'; W='#x'; X='a=~'; Y='b:~/c'; Z='x~'; set | grep -E '^[VWXYZ]='; \
             printf '%q ' '~' '#x' 'a=~' 'x#'; echo; \
             declare -A a; a[\"~\"]=x; a[\"@\"]=y; declare -p a"
        ),
        (
            0,
            "V='~'\nW='#x'\nX='a=~'\nY='b:~/c'\nZ=x~\n\\~ \\#x a=\\~ x# \n\
             declare -A a=([\"@\"]=\"y\" [\"~\"]=\"x\" )"
                .into()
        )
    );
}

#[test]
fn kill_takes_a_signal_joined_to_its_option() {
    // `kill -sHUP` and `kill -n9`, as Bash reads them, were invalid signals `sHUP` and
    // `n9`, and nothing was sent; Bash's jobs tests send them so.
    assert_eq!(
        output(
            "sleep 20 & kill -sHUP $!; echo \"s $?\"; wait $!; echo \"ended $?\"; \
             kill -n0 $$; echo \"n $?\""
        ),
        (0, "s 0\nended 129\nn 0".into())
    );
}

#[test]
fn history_with_a_count_above_its_entries_lists_them_all() {
    // `history 10` with fewer entries panicked (a subtraction below zero), and a second
    // operand was ignored; Bash lists them all, and refuses the second with status 2.
    let result = cash_command()
        .env("HISTFILE", "/dev/null")
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "set -o history\necho a >/dev/null\nhistory 10 >/dev/null; echo \"listed $?\"\n\
             history 10 42; echo \"status $?\"",
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8(result.stdout).unwrap();
    assert_eq!(stdout.replace("\r\n", "\n"), "listed 0\nstatus 2\n");
    assert!(
        String::from_utf8(result.stderr)
            .unwrap()
            .contains("history: too many arguments")
    );
}

#[test]
fn an_assignment_s_subscript_is_expanded_once_after_its_value() {
    // `a[$x]=v` expanded the subscript and then expanded its text again as arithmetic,
    // so a command substitution held in `x`'s value ran: data became code. Bash expands
    // it once and stops on the arithmetic error; and it expands the value first.
    let (status, shown) =
        output("x='$(echo RAN; echo 1)'; a=(zero one); a[$x]=hi; echo \"a: ${a[*]}\"");
    assert_eq!((status, shown.as_str()), (1, ""));
    assert_eq!(
        output("declare -A A; k='$(echo RAN)'; A[$k]=v; echo \"${!A[@]}\""),
        (0, "$(echo RAN)".into())
    );
    let result = cash_command()
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "b[$(echo S >&2; echo 1)]=$(echo V >&2; echo v); declare -p b",
        ])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim_end(),
        "declare -a b=([1]=\"v\")"
    );
    assert_eq!(
        String::from_utf8(result.stderr)
            .unwrap()
            .replace("\r\n", "\n"),
        "V\nS\n"
    );
}

#[test]
fn an_oldpwd_handed_down_is_kept_when_it_names_a_directory() {
    // As in Bash: cash left out every `OLDPWD` it was handed, so one exported under
    // `set -a`, or by a script, never reached a cash it started.
    let scratch = Scratch::new("oldpwd-handed-down");
    let gone = scratch.path().join("gone");
    for (given, shown) in [(scratch.path(), "kept"), (gone.as_path(), "[]")] {
        let result = cash_command()
            .env("OLDPWD", given)
            .args([
                "--noprofile",
                "--norc",
                "-c",
                "[[ -d $OLDPWD ]] && echo kept || echo \"[$OLDPWD]\"",
            ])
            .output()
            .unwrap();
        assert_eq!(String::from_utf8(result.stdout).unwrap().trim_end(), shown);
    }
}

#[test]
fn bash_names_the_shell_s_executable_whatever_zero_is() {
    // As in Bash, `$BASH` is the shell's own program, whole; it was `$0`, so a script
    // that runs `"$BASH"`, as bats does, ran its own name.
    let own = Path::new(CASH)
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .replace('\\', "/");
    let (status, shown) = output("bash -c 'echo \"$BASH\"' named; \"$BASH\" -c 'echo runs'");
    assert_eq!(status, 0);
    let (named, runs) = shown.split_once('\n').unwrap();
    assert!(named.eq_ignore_ascii_case(&own), "{named} is not {own}");
    assert_eq!(runs, "runs");
}

/// Runs `script` and gives its status, standard output and standard error.
fn run_all(script: &str) -> (i32, String, String) {
    let result = cash_command()
        .args(["--noprofile", "--norc", "-c", script])
        .output()
        .unwrap();
    let text = |bytes: Vec<u8>| String::from_utf8(bytes).unwrap().replace("\r\n", "\n");
    (
        result.status.code().unwrap_or(-1),
        text(result.stdout).trim_end().to_string(),
        text(result.stderr),
    )
}

#[test]
fn break_and_continue_leave_only_the_loops_there_are() {
    // Outside a loop they ended the script without a word, and `break 2` in one loop did
    // too; a count of 0 or below looped on; in a function called from a loop it was "not
    // yet implemented", status 99. As in Bash: they say so and go on, a count is held to
    // the loops there are, one below 1 leaves them all with status 1, and a function or a
    // `( )` subshell is inside no loop.
    let (status, stdout, stderr) = run_all(
        "break; echo \"top $?\"; for i in 1 2; do break 2; done; echo \"two $?\"; \
         for i in 1 2 3; do break 0; echo no; done; echo \"zero $?\"; \
         for i in 1 2; do for j in a b; do continue 3; echo no; done; echo no; done; \
         echo \"three $?\"; f() { break; echo \"f goes on $?\"; }; \
         for i in 1 2; do f; echo \"loop $i\"; done; \
         for i in 1; do (break; echo sub); done; \
         trap break USR1; for i in 1 2; do kill -USR1 $$; echo no; done; echo trapped",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "top 0\ntwo 0\nzero 1\nthree 0\nf goes on 0\nloop 1\nf goes on 0\nloop 2\nsub\n\
             trapped"
        )
    );
    assert_eq!(
        stderr
            .matches("only meaningful in a `for', `while', or `until' loop")
            .count(),
        4,
        "{stderr}"
    );
    assert!(
        stderr.contains("break: 0: loop count out of range"),
        "{stderr}"
    );
    // A count that is no number ends the script with status 2.
    let (status, stdout, stderr) = run_all("for i in 1 2; do break x; echo no; done; echo no");
    assert_eq!((status, stdout.as_str()), (2, ""));
    assert!(
        stderr.contains("break: x: numeric argument required"),
        "{stderr}"
    );
}

#[test]
fn printf_reads_its_format_and_conversions_as_bash_does() {
    // `\c` in the format ended all output, `\'` and `\?` stayed, `\u` wanted four
    // digits; `%5b`, `%*q`, `%S` and `%C` were parsing errors and `%ls` counted bytes;
    // `%(...)T` ended at its first `)`; and a bad conversion lost the text before it.
    let (status, stdout, stderr) = run_all(
        "printf 'a\\cb|\\x27q\\x27|\\?|\\u41\\uff|\\n'\n\
         printf '[%5b][%-5b][%.1b][%*b]\\n' ab ab ab 4 ab\n\
         printf '[%*q][%.*q]\\n' 6 'a b' 2 abc\n\
         printf '[%ls][%lc][%.2ls][%S][%C]\\n' ééé ééé ééé ab ab\n\
         TZ=UTC printf '%(%Y (x))T\\n' 0\n\
         printf -v v 'ab%z'; echo \"[$v] $?\"",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "a\\cb|'q'|?|Aÿ|\n[   ab][ab   ][a][  ab]\n[  a\\ b][ab]\n[ééé][é][éé][ab][a]\n\
             1970 (x)\n[ab] 1"
        )
    );
    assert!(
        stderr.contains("printf: `%z': missing format character"),
        "{stderr}"
    );
}

#[test]
fn a_declaration_adopts_what_its_prefix_assigned() {
    // `var=60 export var` exported a variable beside the prefix's, and the value went
    // with the prefix; `local` took no value from a function call's prefix (`a=4 f`);
    // `declare -g` in a function set its caller's local.
    let (status, stdout, _) = run_all(
        "var=10; var=60 export var; declare -p var\n\
         x=1; x=4 declare -r x; declare -p x\n\
         y=5 readonly y; declare -p y\n\
         f() { z=y typeset z; echo \"|$z|\"; }; f\n\
         g() { local -i a; a+=3; echo $a; }; a=4 g\n\
         h() { local w; k; echo \"h: w=$w\"; }; k() { declare -g w=one; }; h; echo \"global: w=$w\"",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "declare -x var=\"60\"\ndeclare -rx x=\"4\"\ndeclare -rx y=\"5\"\n|y|\n7\nh: w=\n\
             global: w=one"
        )
    );
    // In POSIX mode what a special builtin is given before it stays.
    assert_eq!(
        output("set -o posix; y=2; y=5 eval :; echo $y; z=1 :; echo $z"),
        (0, "5\n1".into())
    );
}

#[test]
fn a_read_only_variable_stays_as_it_is() {
    // A prefix assignment to it, a local over it, an element of a read-only associative
    // array, and `coproc` of its name all changed it, or shadowed it, in silence.
    let (status, stdout, stderr) = run_all(
        "readonly x=4; x=8 echo ran\n\
         readonly var=outside; f() { local var=inside; echo \"in: $var\"; }; f\n\
         readonly RO=1; coproc RO { :; }; echo \"RO=$RO\"\n\
         declare -rA A=([k]=v); unset 'A[k]'; declare -p A; A[k]=w; echo not here",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (1, "ran\nin: outside\nRO=1\ndeclare -Ar A=([k]=\"v\" )")
    );
    for said in [
        "x: readonly variable",
        "local: var: readonly variable",
        "RO: readonly variable",
        "unset: A: cannot unset: readonly variable",
        "A: readonly variable",
    ] {
        assert!(stderr.contains(said), "{said}: {stderr}");
    }
    // In POSIX mode the prefix assignment ends the script.
    assert_eq!(
        run_all("set -o posix; readonly x=4; x=8 echo ran; echo no").0,
        1
    );
    // A caller's read-only local may be shadowed, as bats-support's `local -ir width` is.
    assert_eq!(
        output("f() { local -r q=1; g; echo \"f: $q\"; }; g() { local q=2; echo \"g: $q\"; }; f"),
        (0, "g: 2\nf: 1".into())
    );
}

#[test]
fn return_and_exit_read_their_status_as_bash_does() {
    // `return -1` was refused as an option and `return abc` went on with the function;
    // Bash's statuses are taken modulo 256, a word that is no number is 2, and a second
    // word ends a `-c` string with status 1 (a script goes on at its next line, below).
    let (status, stdout, stderr) = run_all(
        "f() { return -1; }; f; echo \"$?\"; g() { return abc; echo no; }; g; echo \"$?\"; \
         h() { return -- 300; }; h; echo \"$?\"; k() { return 2 3; }; k; echo no",
    );
    assert_eq!((status, stdout.as_str()), (1, "255\n2\n44"));
    assert!(
        stderr.contains("return: abc: numeric argument required")
            && stderr.contains("return: too many arguments"),
        "{stderr}"
    );
    assert_eq!(run_all("exit abc").0, 2);
    assert_eq!(run_all("exit -- -2").0, 254);
    assert_eq!(run_all("exit 2 3; echo no").0, 1);
}

#[test]
fn complete_p_prints_a_keeping_pattern_with_its_bang() {
    // The `!` of `-X '!*.txt'` was dropped, so the spec printed did not read back.
    let (status, stdout) =
        output("complete -X '!*.txt' -f foo; complete -X '*.o' -f bar; complete -p foo bar");
    assert_eq!(
        (status, stdout.as_str()),
        (0, "complete -f -X '!*.txt' foo\ncomplete -f -X '*.o' bar")
    );
}

#[test]
fn the_history_list_is_there_with_history_off_and_takes_ranges() {
    // As Git Bash 5.3: `history -s` in a script added nothing, `fc -l` said history was
    // not enabled, `history -d 2-3` was refused, `history -p '!!:z'` printed the line,
    // and `history -w /dev/null` looked for `C:/dev/null`.
    let (status, stdout) = output(
        "fc -l; echo \"l $?\"; history -s foo; history; history -c; \
         for w in a b c d e; do history -s $w; done; history -d 2-3; history -d -1; history; \
         history -d 9 2>/dev/null; echo \"d $?\"; \
         history -p '!!:z' 2>/dev/null; echo \"p $?\"; history -w /dev/null; echo \"w $?\"",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (0, "l 0\n    1  foo\n    1  a\n    2  d\nd 1\np 1\nw 0")
    );
}

#[test]
fn declare_keeps_a_readonly_function_one() {
    // `declare -f +r f` printed the function, status 0; Bash refuses, status 1, and for a
    // function that is not read-only does nothing.
    let (status, stdout) = output(
        "f() { :; }; declare -fr f; declare -f +r f 2>/dev/null; echo \"s $?\"; \
         g() { :; }; declare -f +r g; echo \"g $?\"",
    );
    assert_eq!((status, stdout.as_str()), (0, "s 1\ng 0"));
}

#[test]
fn compgen_k_lists_the_reserved_words_in_bashs_order() {
    // They came from a set, in an order that changed from run to run.
    let (status, stdout) = output("compgen -k | tr '\n' ' '");
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "if then else elif fi case esac for select while until do done in function time { } ! [[ ]] coproc"
        )
    );
}

#[test]
fn a_trailing_backslash_is_a_character_and_a_loop_name_must_be_one() {
    // `echo a\` at the end of the text printed `a`: the newline added after it made a
    // line continuation. `case x in esac)` ran; Bash ends the case at `esac`. `for 1 in`
    // and `select 2 in` ran the loop; Bash says the name is none, status 1.
    let (_, stdout) = output("echo a\\");
    assert_eq!(stdout, "a\\");
    let (status, _) = output("case x in esac) echo hit;; esac");
    assert_eq!(status, 2);
    let (status, stdout) = output(
        "for 1 in a; do echo in; done; echo \"s $?\"; select 2 in a; do :; done </dev/null; \
         echo \"s $?\"",
    );
    assert_eq!((status, stdout.as_str()), (0, "s 1\ns 1"));
}

#[test]
fn text_right_after_a_lists_parenthesis_makes_one_word() {
    // `let a=(4*3)/2` was the list `a=(4*3)` and a word `/2`; Bash reads one word.
    let (status, stdout) = output(
        "let a=(4*3)/2; echo \"$a\"; c=(x y)z; echo \"${c[@]}\" \"${#c[@]}\"; d=(p q); echo \"${d[1]}\"",
    );
    assert_eq!((status, stdout.as_str()), (0, "6\n(x y)z 1\nq"));
}

#[test]
fn a_current_shell_substitution_holds_groups_and_ends_as_bash_s_does() {
    // A function's body inside ended it at its `}`; `}` after a newline or a `)` was a bad
    // substitution; `exit` in it ended only it, and `return` at the top level was refused.
    let (status, stdout) = output(
        "x=${ g() { echo in; }; g; }; echo \"[$x]\"; y=${ echo a\n}; z=${ (echo b)}; \
         echo \"[$y][$z]\"; r=${ return 2; echo no; }; echo \"[$r] $?\"; v=${ exit 4; }; echo after",
    );
    assert_eq!((status, stdout.as_str()), (4, "[in]\n[a][b]\n[] 2"));
}

#[test]
fn declarations_and_options_left_from_the_arrays_phase_match_bash() {
    // `declare n[sub]=v` wrote element 0; `set -u` let `$(( a[0] ))` of an unset `a` be
    // 0; an associative list split `$v`; `export -a` was refused; `localvar_inherit` and
    // `set -k` did nothing.
    let (status, stdout) = output(
        "sub=2; declare n[sub]=v; declare -p n; declare -A A; v='p q'; A=($v); declare -p A; \
         export -a e; echo \"e $?\"; shopt -s localvar_inherit; declare -i x=1; \
         f() { local x; x=2+3; echo \"$x\"; }; f; set -k; g() { echo \"[$y] $*\"; }; g z y=5 w; \
         set +k; set -u; unset a; echo $(( a[0] )); echo never",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            127,
            "declare -a n=([2]=\"v\")\ndeclare -A A=([\"p q\"]=\"\" )\ne 0\n5\n[5] z w"
        )
    );
}

#[test]
fn arithmetic_reads_assignment_and_quotes_as_bash_does() {
    // An assignment was an operand of `?:`; a single quote was removed before the
    // expression was read; a variable holding `b[$d]` was not expanded; `-v A[$k]` cut a
    // key holding `]`.
    let (status, stdout) = output(
        "x=1; (( 1 ? 20 : x+=2 )) 2>/dev/null; echo \"s $? x $x\"; (( 1 ? x=3 : 4 )); \
         echo \"x $x\"; ( echo $(( '1' + 2 )) ) 2>/dev/null; echo \"q $?\"; declare -A A; \
         (( A['k']=1 )); echo \"k ${A[k]}\"; b=(5 6 7 8 9); d=3; v=\"b[\\$d]\"; \
         echo \"v $(( v ))\"; k='a]b'; A[$k]=1; [[ -v A[$k] ]] && echo set; \
         [[ -v A[a] ]] || echo unset",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (0, "s 1 x 1\nx 3\nq 1\nk 1\nv 8\nset\nunset")
    );
}

#[test]
fn a_list_element_with_plus_equals_appends_to_its_element() {
    // `[k]+=v` in a list was a word, an element of its own; it appends to element `k` as
    // it is by then, in the array or earlier in the list, and adds for an integer.
    let (status, stdout) = output(
        "a=(x y); a+=([0]+=z); echo \"${a[*]}\"; b=([0]=x [0]+=y [3]+=w); echo \"${b[*]}\"; \
         declare -ai d=(1 2); d+=([0]+=5 [1]+=3*2); echo \"${d[*]}\"; \
         declare -A A=([k]=v); A+=([k]+=w); echo \"${A[k]}\"; f=('[0]+=q'); echo \"${f[*]}\"; \
         g() { local -a l=(p [0]+=q); echo \"${l[*]}\"; }; g; declare -f g | sed -n 3p",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "xz y\nxy w\n6 8\nvw\n[0]+=q\npq\n    local -a l=(p [0]+=q);"
        )
    );
}

#[test]
fn too_many_arguments_abandons_the_command_as_bash_53_does() {
    // `break 1 2`, `continue`, `shift`, `return` and `exit` with a second operand were
    // usage errors, or ended the script; Bash 5.3 leaves every `eval` and sourced file,
    // abandons the top-level command, and goes on at the next line with status 2. A
    // subshell ends with 2, a command substitution with 1; `exit x` goes on.
    let (stdout, stderr) = script_output(
        "too-many-arguments",
        "for i in 1 2; do shift 1 2; echo no; done; echo no\necho \"a $?\"\n\
         f() { for i in 1; do continue 1 2; done; echo no; }; f; echo no\necho \"b $?\"\n\
         eval 'shift 1 2'; echo no\necho \"c $?\"\n\
         x=$(exit 1 2; echo no); echo \"d $? [$x]\"\n( return 1 2 ); echo \"e $?\"\n\
         exit x; echo \"f $?\"\n",
    );
    assert_eq!(stdout, "a 2\nb 2\nc 2\nd 1 []\ne 2\nf 2");
    assert!(
        stderr.contains("line 1: shift: too many arguments")
            && stderr.contains("line 3: continue: too many arguments")
            && stderr.contains("line 9: exit: x: numeric argument required"),
        "{stderr}"
    );
    // A `-c` string ends there, status 1.
    assert_eq!(output("shift 1 2; echo no\necho next"), (1, String::new()));
}

#[test]
fn builtins_refuse_options_and_operands_as_bash_does() {
    // `set -Z x` set `$1` to `-Z`; `set -o nosuch` said nothing; `eval -Z` and
    // `command -- -Z` swapped which word was an option; `disown -Z` took it for a job;
    // `mapfile` took any name; `umask` refused the operands Bash ignores; `cd ''`, `cd a
    // b`, `pushd a b` and `history x` were said in clap's or Rust's words; `declare -f -a`
    // printed the function.
    let (status, stdout, stderr) = run_all(
        "set -Z x 2>/dev/null; echo \"d $? [$1]\"; set -o nosuch; echo \"e $?\"; \
         eval -Z 2>/dev/null; echo \"f $?\"; command -- -Z 2>/dev/null; echo \"g $?\"; \
         disown -Z 2>/dev/null; echo \"h $?\"; mapfile -- -Z </dev/null; echo \"i $?\"; \
         mapfile '' </dev/null; echo \"j $?\"; umask 077 2 3; echo \"k $? $(umask)\"; \
         cd ''; echo \"l $?\"; cd a b; echo \"m $?\"; pushd a b 2>/dev/null; echo \"n $?\"; \
         history x; echo \"o $?\"; g() { :; }; declare -f -a g; echo \"p $?\"",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "d 2 []\ne 2\nf 2\ng 127\nh 2\ni 1\nj 2\nk 0 0077\nl 1\nm 2\nn 1\no 2\np 1"
        )
    );
    for said in [
        "set: nosuch: invalid option name",
        "mapfile: `-Z': not a valid identifier",
        "mapfile: empty array variable name",
        "cd: null directory",
        "cd: too many arguments",
        "history: x: numeric argument required",
        "declare: -a: invalid option",
    ] {
        assert!(stderr.contains(said), "{said}: {stderr}");
    }
}

#[test]
fn a_length_takes_its_parameter_as_bash_does() {
    // `${#/}` and `${#^}` were `$#` with an operator; Bash finds no parameter to measure.
    // `${##x}` and `${#@Q}` were refused; they are `$#` with an operator.
    let (status, stdout) = output(
        "set -- a b c; for e in '${#/}' '${#%}' '${#=}' '${#+}' '${#^}' '${#,}' '${#~}' \
         '${##x}' '${###}' '${#@Q}' '${#/x}' '${#?}'; do \
         (eval \"echo \\\"$e\\\"\") 2>/dev/null || echo bad; done",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (0, "bad\nbad\nbad\nbad\nbad\nbad\nbad\n3\n3\n'3'\n3\n1")
    );
}

#[test]
fn a_script_read_from_standard_input_follows_exec_of_fd_0() {
    // The rest of a script read from standard input comes from the shell's fd 0 as it is
    // then, as in Bash: after `exec 0<file` from the file, and a closed one ends it. It
    // came from the input the shell started with.
    use std::io::Write as _;

    let scratch = Scratch::new("exec-stdin");
    std::fs::write(
        scratch.path().join("cmds.txt"),
        "echo from-file\nread y\nthe-files-line\necho \"[$y]\"\n",
    )
    .unwrap();
    for (script, expected) in [
        ("read x\nhello\necho \"[$x]\"\n", "[hello]"),
        (
            "exec 0<cmds.txt\necho not-read\n",
            "from-file\n[the-files-line]",
        ),
        ("echo one\nexec 0<&-\necho not-read\n", "one"),
    ] {
        let mut child = cash_command()
            .args(["--noprofile", "--norc"])
            .current_dir(scratch.path())
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(script.as_bytes())
            .unwrap();
        let result = child.wait_with_output().unwrap();
        let stdout = String::from_utf8(result.stdout)
            .unwrap()
            .replace("\r\n", "\n");
        assert_eq!(
            stdout.trim_end(),
            expected,
            "{script}: {}",
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[test]
fn a_here_document_closed_by_a_parenthesis_is_warned_of() {
    // `$(cat <<EOF` ... `EOF)` was taken in silence; Bash warns that the here-document was
    // delimited by end-of-file, with the line of the `)` and the line it began on.
    let (stdout, stderr) = script_output(
        "here-document-parenthesis",
        "echo start\nx=$(cat <<EOF\nhello\nEOF)\necho \"[$x]\"\ny=$(cat <<-'END'\n\ttwo\n\tEND)\n\
         echo \"[$y]\"\n",
    );
    assert_eq!(stdout, "start\n[hello]\n[two]");
    assert!(
        stderr.contains(
            "line 4: warning: here-document at line 2 delimited by end-of-file (wanted `EOF')"
        ) && stderr.contains(
            "line 8: warning: here-document at line 6 delimited by end-of-file (wanted `END')"
        ),
        "{stderr}"
    );
}

/// Runs cash with `args` after `--noprofile --norc`, `env` added to its environment, in
/// `directory` when one is given: its status, standard output trimmed, and standard error.
fn start(args: &[&str], env: &[(&str, &str)], directory: Option<&Path>) -> (i32, String, String) {
    let mut command = cash_command();
    command
        .args(["--noprofile", "--norc"])
        .args(args)
        .envs(env.iter().copied());
    if let Some(directory) = directory {
        command.current_dir(directory);
    }
    let result = command.output().unwrap();
    let text = |bytes: Vec<u8>| String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
    (
        result.status.code().unwrap_or(-1),
        text(result.stdout).trim_end().to_owned(),
        text(result.stderr),
    )
}

#[test]
fn starting_options_are_taken_as_bash_takes_them() {
    // `+B`, `+o braceexpand` and the `set` letters clap had no flag for were refused; `-O`
    // and `-o` with a name they do not know were passed over; `+O` was taken anywhere on
    // the line, a script's operand too.
    assert_eq!(start(&["+B", "-c", "echo {a,b}"], &[], None).1, "{a,b}");
    assert_eq!(
        start(&["+o", "braceexpand", "-c", "echo {a,b}"], &[], None).1,
        "{a,b}"
    );
    assert_eq!(start(&["-aE", "-c", "echo $-"], &[], None).1, "ahBEc");
    assert_eq!(start(&["+eu", "-c", "echo $-"], &[], None).1, "hBc");
    assert_eq!(
        start(&["-c", "echo \"$1\"", "x", "+Ofoo"], &[], None).1,
        "+Ofoo"
    );
    let (status, stdout, stderr) = start(&["-O", "badopt", "-c", "echo ran"], &[], None);
    assert_eq!((status, stdout.as_str()), (2, ""));
    assert!(
        stderr.contains("line 0: badopt: invalid shell option name"),
        "{stderr}"
    );
    let (status, _, stderr) = start(&["-o", "nosuch", "-c", "echo ran"], &[], None);
    assert_eq!(status, 2);
    assert!(stderr.contains(": nosuch: invalid option name"), "{stderr}");
}

#[test]
fn shellopts_bashopts_and_bash_argv0_are_taken_from_the_environment() {
    // They were dropped: the options were not set, nor `$0` named; `SHELLOPTS=x` was taken
    // in silence.
    let (_, stdout, _) = start(
        &[
            "-c",
            "echo *; env | grep -c ^SHELLOPTS=; shopt -q nullglob && echo null",
        ],
        &[("SHELLOPTS", "noglob"), ("BASHOPTS", "nullglob")],
        None,
    );
    assert_eq!(stdout, "*\n1\nnull");
    let (_, stdout, stderr) = start(&["-c", "echo *"], &[("SHELLOPTS", "nosuch:noglob")], None);
    assert_eq!(stdout, "*");
    assert!(
        stderr.contains("line 0: nosuch: invalid option name"),
        "{stderr}"
    );
    assert_eq!(
        start(&["-c", "echo $0"], &[("BASH_ARGV0", "renamed")], None).1,
        "renamed"
    );
    let (status, stdout, stderr) = start(&["-c", "SHELLOPTS=x; echo no"], &[], None);
    assert_eq!((status, stdout.as_str()), (1, ""));
    assert!(stderr.contains("SHELLOPTS: readonly variable"), "{stderr}");
}

#[test]
fn a_script_operand_is_looked_for_on_path() {
    // A script not in the current directory was not found; Bash looks for it on `PATH`,
    // `$0` the name given. One found nowhere is said after the shell's name, not the
    // script's with `line 1`.
    let scratch = Scratch::new("script-on-path");
    let bin = scratch.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    std::fs::write(bin.join("found.sh"), "echo \"found $0\"\n").unwrap();
    let path = format!(
        "{};{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let (status, stdout, _) = start(&["found.sh"], &[("PATH", &path)], Some(scratch.path()));
    assert_eq!((status, stdout.as_str()), (0, "found found.sh"));
    let (status, _, stderr) = start(&["nosuch.sh"], &[], Some(scratch.path()));
    assert_eq!(status, 127);
    assert!(
        stderr.ends_with(": nosuch.sh: No such file or directory\n") && !stderr.contains("line"),
        "{stderr}"
    );
}

#[test]
fn a_renamed_cash_runs_sh_scripts_itself() {
    // A cash named `bash.exe` sent `#!/bin/sh` scripts to the `cash` on `PATH`.
    let scratch = Scratch::new("renamed-cash");
    let renamed = scratch.path().join("bash.exe");
    if std::fs::hard_link(CASH, &renamed).is_err() {
        std::fs::copy(CASH, &renamed).unwrap();
    }
    std::fs::write(scratch.path().join("s"), "#!/bin/sh\necho \"$BASH\"\n").unwrap();
    let result = std::process::Command::new(&renamed)
        .args(["--noprofile", "--norc", "-c", "./s"])
        .env("CASH_NO_OFFER", "1")
        .current_dir(scratch.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(stdout.trim_end().ends_with("bash.exe"), "{stdout}");
}

#[test]
fn pretty_print_writes_the_script_as_bash_does() {
    // `--pretty-print` was refused. A pipe was written `|cat` and a duplication `2>& 1`,
    // here and in `declare -f`.
    let scratch = Scratch::new("pretty-print");
    std::fs::write(
        scratch.path().join("p.sh"),
        "if true; then echo   hi; fi\nwhile false; do :; done; echo after\n\
         echo a 2>&1 | cat <&3\ntrue & echo b\n",
    )
    .unwrap();
    let (status, stdout, _) = start(&["--pretty-print", "p.sh"], &[], Some(scratch.path()));
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "if true; then\n    echo hi;\nfi\nwhile false; do\n    :;\ndone; echo after\n\
             echo a 2>&1 | cat 0<&3\ntrue & echo b"
        )
    );
    assert_eq!(
        start(&["--pretty-print", "-c", "echo hi"], &[], None),
        (0, String::new(), String::new())
    );
}

#[test]
fn posixly_correct_and_posix_mode_follow_each_other() {
    // POSIXLY_CORRECT neither turned POSIX mode on nor followed it.
    let mode = "shopt -o posix | tr -d ' \\t'";
    assert_eq!(
        start(
            &["-c", &format!("{mode}; echo \"$POSIXLY_CORRECT\"")],
            &[("POSIXLY_CORRECT", "1")],
            None
        )
        .1,
        "posixon\n1"
    );
    assert_eq!(
        start(
            &[
                "-c",
                "set -o posix; echo \"${POSIXLY_CORRECT-unset}\"; set +o posix; \
                 echo \"${POSIXLY_CORRECT-unset}\""
            ],
            &[],
            None
        )
        .1,
        "y\nunset"
    );
    assert_eq!(
        start(
            &[
                "-c",
                &format!("POSIXLY_CORRECT=1; {mode}; unset POSIXLY_CORRECT; {mode}")
            ],
            &[],
            None
        )
        .1,
        "posixon\nposixoff"
    );
    assert_eq!(
        start(&["--posix", "-c", "echo \"$POSIXLY_CORRECT\""], &[], None).1,
        "y"
    );
}

#[test]
fn a_special_builtins_error_ends_a_posix_shell() {
    // A redirection on `:`, `unset` of a read-only, `return` outside a function, a bad
    // name to `readonly` or `export`, and a usage error went on in POSIX mode; Bash ends
    // there. `trap` with a name that is no signal, and `command eval` with a syntax
    // error, ended it; Bash goes on.
    for (script, status) in [
        (": > /nonexistent/x", 1),
        ("readonly r=1; unset r", 1),
        ("return", 2),
        ("readonly 1bad=2", 1),
        ("export 1bad", 1),
        ("set -Z", 2),
        ("set -o nosuch", 2),
        ("unset -v 1bad", 1),
    ] {
        let (got, stdout, _) = start(
            &["-c", &format!("set -o posix; {script}; echo went-on")],
            &[],
            None,
        );
        assert_eq!((got, stdout.as_str()), (status, ""), "{script}");
    }
    for script in [
        "trap 'echo t' SIGNOSIG",
        "command eval '( '",
        "command : > /nonexistent/x",
        "command unset -x",
    ] {
        let (_, stdout, _) = start(
            &["-c", &format!("set -o posix; {script}; echo went-on")],
            &[],
            None,
        );
        assert_eq!(stdout, "went-on", "{script}");
    }
    // Out of POSIX mode they go on, and `trap` acts on the names that are signals.
    assert_eq!(
        start(
            &[
                "-c",
                "readonly r=1; unset r; trap 'echo t' NOSIG INT; trap -p INT"
            ],
            &[],
            None
        )
        .1,
        "trap -- 'echo t' SIGINT"
    );
}

#[test]
fn cash_started_as_sh_and_a_substitution_under_set_e_are_posix() {
    // A cash named `sh`, which `#!/bin/sh` and `sh -c` start, was not in POSIX mode; a
    // command substitution there did not take `set -e`.
    let scratch = Scratch::new("cash-as-sh");
    let sh = scratch.path().join("sh.exe");
    if std::fs::hard_link(CASH, &sh).is_err() {
        std::fs::copy(CASH, &sh).unwrap();
    }
    let result = std::process::Command::new(&sh)
        .args([
            "--noprofile",
            "--norc",
            "-c",
            "shopt -o posix | tr -d ' \\t'",
        ])
        .env("CASH_NO_OFFER", "1")
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8_lossy(&result.stdout).trim_end(),
        "posixon"
    );
    assert_eq!(
        start(
            &["-c", "set -o posix -e; x=$(false; echo in); echo \"[$x]\""],
            &[],
            None
        ),
        (1, String::new(), String::new())
    );
}

#[test]
fn declare_f_prints_words_substitutions_and_elif_as_bash_does() {
    // A word's second line was indented, so `x="a<newline>b"` read back held the indent and
    // a here-document in `$(...)` never ended; a process substitution was `<(( z ))`;
    // `$( echo  hi )` was printed as written; `elif` was `elif` (Bash: `else` and `if`).
    let (status, stdout) = output(
        "f() {\n  x=\"a\nb\"\n  y=$(cat <<EOF\nbody\nEOF\n)\n  printf '[%s][%s]' \"$x\" \"$y\"\n}\n\
         eval \"$(declare -f f)\"; f; echo\n\
         g() { a=$( echo  hi ); b=$( (echo n) ); cat <(z); \
         if p; then q; elif r; then s; else t; fi; }; declare -f g",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "[a\nb][body]\ng () \n{ \n    a=$(echo hi);\n    b=$( ( echo n ));\n    cat <(z);\n    \
             if p; then\n        q;\n    else\n        if r; then\n            s;\n        else\n            \
             t;\n        fi;\n    fi\n}"
        )
    );
}

#[test]
fn a_subscript_expanded_already_is_not_expanded_again() {
    // `(( A[\$k]++ ))` counted the key `x`; Bash counts `$k`, as the expression was
    // expanded once already. `let` and a variable's value expand it.
    let (status, stdout) = output(
        "declare -A A=([x]=7); k=x; echo $(( A[\\$k] )) $(( A[$k] )); (( A[\\$k]++ )); \
         echo \"${A[\\$k]}\"; let 'A[$k]=8'; echo \"${A[x]}\"; v='A[$k]'; echo $(( v ))",
    );
    assert_eq!((status, stdout.as_str()), (0, "0 7\n1\n8\n8"));
}

#[test]
fn a_reference_to_an_element_with_an_arithmetic_subscript_takes_read() {
    // `read r` with `r` naming `X[i+1]` wrote element 0; `read -a r` and `mapfile r`
    // through it replaced the array or did nothing, where Bash refuses them.
    let (status, stdout, stderr) = run_all(
        "X=(a b c); i=0; declare -n r='X[i+1]'; read r <<< new; printf -v r %s two; \
         declare -n e='X[-1]'; read e <<< last; declare -p X; read -a r <<< 'p q'; \
         echo \"s $?\"; mapfile r <<< p; echo \"s $?\"; declare -p X",
    );
    assert_eq!(
        (status, stdout.as_str()),
        (
            0,
            "declare -a X=([0]=\"a\" [1]=\"two\" [2]=\"last\")\ns 1\ns 1\n\
             declare -a X=([0]=\"a\" [1]=\"two\" [2]=\"last\")"
        )
    );
    assert!(
        stderr.contains("read: `X[i+1]': not a valid identifier")
            && stderr.contains("mapfile: `X[i+1]': not a valid identifier"),
        "{stderr}"
    );
}

#[test]
fn history_numbers_keep_counting_and_set_h_expands_script_lines() {
    // History numbers began again at 1 once HISTSIZE dropped the oldest; Bash's keep
    // counting. `set -H` left `!!` in a script's lines; Bash expands them, says the
    // expanded line on standard error, and drops a line whose event is not found.
    let (stdout, stderr) = script_output(
        "history-numbers",
        "HISTFILE=/dev/null\nset -o history\nHISTSIZE=3\necho a > /dev/null\n\
         echo b > /dev/null\necho c > /dev/null\nhistory\necho \"HISTCMD=$HISTCMD\"\n\
         set -H\necho x\necho !ech; echo z\necho !nosuch; echo same\necho next\n",
    );
    assert_eq!(
        stdout,
        "    3  echo b > /dev/null\n    4  echo c > /dev/null\n    5  history\nHISTCMD=6\n\
         x\necho x\nz\nnext"
    );
    assert!(
        stderr.contains("echo echo x; echo z")
            && stderr.contains("line 12: !nosuch: event not found"),
        "{stderr}"
    );
}

#[test]
fn dash_c_takes_no_word_of_its_own() {
    // `-c -ex cmd` was refused, and `-ec -c cmd` ran `-c` as the command: Bash's `-c`
    // takes no word, the command is the first word after the options.
    assert_eq!(start(&["-c", "-ex", "echo $-"], &[], None).1, "ehxBc");
    assert_eq!(start(&["-ec", "-c", "echo $-"], &[], None).1, "ehBc");
}

#[test]
fn compound_assignments_trace_and_shopt_o_lists_as_bash_does() {
    // A list was traced as the values it took, and a declaration's as `c='(q x y)'`;
    // Bash traces a list as written, and a declaration's on a line of its own before the
    // command. `shopt -o` listed the names 20 wide; Bash lists them 15 wide.
    let (_, stdout, stderr) = run_all(
        "v=q; set -x; a=($v \"$v\" [2]=$v); declare -a c=($v 'x y'); declare -A A=([k]=v); \
         set +x; shopt -o | head -1",
    );
    assert_eq!(stdout, "allexport      \toff");
    assert!(
        stderr.contains("+ a=($v \"$v\" [2]=$v)\n")
            && stderr.contains("+ c=('q' 'x y')\n+ declare -a c\n")
            && stderr.contains("+ A=(['k']='v')\n+ declare -A A\n"),
        "{stderr}"
    );
}

#[test]
fn compgen_b_and_complete_p_list_in_the_same_order_every_run() {
    // Both listed in a hash map's order, which changed from run to run.
    let (_, builtins) = output("compgen -b");
    let names: Vec<&str> = builtins.lines().collect();
    let mut sorted = names.clone();
    sorted.sort_unstable();
    assert_eq!(names, sorted);
    assert_eq!(names.first().copied(), Some("."));
    assert_eq!(
        output("complete -W z c; complete -W x a; complete -W y b; complete -p").1,
        "complete -W 'x' a\ncomplete -W 'y' b\ncomplete -W 'z' c"
    );
}

#[test]
fn return_traps_and_caller_see_the_lines_and_frames_bash_shows() {
    // A function's body that ran to its end leaves LINENO at its `{` (it was the last
    // command's line); a `.` runs its RETURN trap once the file's frame is gone, on the
    // line of the `.` (it saw the file's last line and the frame). `caller` with a frame
    // and no caller prints `0 NULL` (it printed nothing and failed).
    let (stdout, _) = script_output(
        "return-traps",
        "trap 'echo \"RETURN $LINENO ${FUNCNAME[0]:-$(caller)}\"' RETURN\n\
         set -o functrace\n\
         f() {\n    echo one\n}\n\
         f\n\
         k() { echo k; return; }\n\
         k\n\
         d=$(mktemp -d); printf 'echo sourced\\n' > \"$d/src.sh\"\n\
         . \"$d/src.sh\"; rm -r \"$d\"\n\
         trap - RETURN\n\
         caller\n\
         echo \"caller $?\"\n",
    );
    assert_eq!(
        stdout,
        "one\nRETURN 3 f\nk\nRETURN 7 k\nsourced\nRETURN 10 0 NULL\n0 NULL\ncaller 0"
    );
    assert_eq!(output("f() { caller; }; f; caller; echo $?").1, "1 NULL\n1");
    // A trap sees `[[ ]]` as the command last run, as a DEBUG trap did already.
    assert_eq!(
        output("trap 'echo \"$BASH_COMMAND\"' EXIT; g() { [[ a == a ]]; }; g").1,
        "[[ a == a ]]"
    );
}

#[test]
fn job_specs_run_as_commands_and_bg_resumes_as_bash_does() {
    // `%1` and `%name` were "command not found", `fg %name` "no such job", and `bg` of a
    // running job "not yet implemented"; `bg` named no job it resumed.
    let (stdout, stderr) = script_output(
        "job-specs",
        "%1\n\
         echo \"without job control $?\"\n\
         set -m\n\
         sleep 0.2 &\n%1\n\
         sleep 0.2 &\n%sleep\n\
         sleep 0.2 &\nfg %?0.2\n\
         sleep 2 &\n\
         bg\n\
         %1 &\n\
         kill -STOP %1\n\
         sleep 0.1\n\
         bg %1\n\
         kill %1\n\
         wait\n\
         echo end\n",
    );
    assert_eq!(
        stdout,
        "without job control 1\nsleep 0.2\nsleep 0.2\nsleep 0.2\n[1]+ sleep 2 &\nend"
    );
    assert!(stderr.contains("line 1: fg: no job control"), "{stderr}");
    assert!(
        stderr.contains("line 11: bg: job 1 already in background"),
        "{stderr}"
    );
    assert!(
        stderr.contains("line 12: bg: job 1 already in background"),
        "{stderr}"
    );
}

#[test]
fn a_process_substitution_is_bang_and_wait_waits_for_it() {
    // `$!` was left as it was, so `exec 3> >(tee log); ...; wait $!` could not wait for the
    // substitution to write its last.
    let scratch = Scratch::new("procsub-bang");
    let (_, stdout) = output_in(
        scratch.path(),
        ": <(exit 3); p=$!; wait $p; echo \"status $?\"; [ \"$p\" = \"$!\" ] && echo same\n\
         exec 3> >(sleep 0.3; cat > out.txt); echo hi >&3; exec 3>&-; wait $!; cat out.txt",
    );
    assert_eq!(stdout, "status 3\nsame\nhi");
}

#[test]
fn a_signal_a_job_sends_the_shell_is_the_shells() {
    // A job runs inside the shell's process, and its `kill $$` ran the trap in the job:
    // what the trap set was lost, `wait` went on, and a watchdog's `kill -TERM $$` ended
    // the job, not the script.
    let started = std::time::Instant::now();
    let (status, stdout) = output(
        "trap 'got=$((got+1))' USR1; got=0\n\
         ( kill -USR1 $$ ) &\n\
         wait; s=$?\n\
         echo \"got=$got wait=$((s > 128))\"\n\
         ( kill -USR1 $$ ); echo \"got=$got\"\n\
         trap - USR1\n\
         ( sleep 0.3; kill -TERM $$ ) &\n\
         sleep 20\n\
         echo not reached",
    );
    assert_eq!(stdout, "got=1 wait=1\ngot=2");
    assert_eq!(status, 143);
    assert!(started.elapsed() < std::time::Duration::from_secs(15));
}

#[test]
fn a_copy_of_the_shell_has_a_bashpid_of_its_own() {
    // A subshell, a substitution and a job run inside the shell's process, and their
    // `$BASHPID` was `$$`: `[[ $BASHPID != $$ ]]` found no subshell, and `kill $BASHPID`
    // in one could not be told from `kill $$`. A job's is its `$!`, as in Bash.
    let (_, stdout) = output(
        "[[ $BASHPID == $$ ]] && echo top; ( [[ $BASHPID != $$ ]] && echo sub )\n\
         echo \"$([[ $BASHPID != $$ ]] && echo substitution)\"\n\
         { echo \"$BASHPID\"; } & wait; echo \"$!\"",
    );
    let lines: Vec<&str> = stdout.lines().collect();
    assert_eq!(
        lines.get(..3),
        Some(&["top", "sub", "substitution"][..]),
        "{stdout}"
    );
    assert_eq!(lines.get(3), lines.get(4), "{stdout}");
}
