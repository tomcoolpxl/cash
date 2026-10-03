//! Comprehensive parameter and edge-case integration tests for shell built-ins.
//!
//! Real end-to-end integration tests that invoke the actual `cash.exe` binary
//! using real Windows pipes, real processes, exit codes, and stdout/stderr assertions.

#![allow(
    clippy::tests_outside_test_module,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::needless_raw_string_hashes,
    clippy::single_char_pattern,
    clippy::literal_string_with_formatting_args,
    reason = "Integration tests test the compiled binary and assert loudly on failure."
)]

use crate::common::{CASH, Output, Scratch, cash_command, output_of};

fn cash(script: &str) -> Output {
    run(&["-c", script])
}

fn cash_login(script: &str) -> Output {
    run(&["-l", "-c", script])
}

fn run(args: &[&str]) -> Output {
    output_of(cash_command().args(args).env("HISTFILE", ""))
}

/// Scratch sandbox directory in %TEMP% that cleans up after itself.
struct Sandbox {
    root: Scratch,
}

impl Sandbox {
    fn new(name: &str) -> Self {
        Self {
            root: Scratch::new(&format!("builtin-test-{name}")),
        }
    }

    fn run(&self, script: &str) -> Output {
        output_of(
            cash_command()
                .current_dir(self.root.path())
                .args(["-c", script])
                .env("HISTFILE", ""),
        )
    }
}

// ===========================================================================
// 1. hostname parameter tests
// ===========================================================================

#[test]
fn hostname_short_matches_bare_hostname() {
    let bare = cash("hostname").stdout;
    let short = cash("hostname -s").stdout;
    let short_long = cash("hostname --short").stdout;
    assert_eq!(bare, short);
    assert_eq!(bare, short_long);
}

#[test]
fn hostname_conflicting_flags_fail() {
    let out = cash("hostname -s -f");
    assert_ne!(out.code, 0, "conflicting flags should fail");
    assert!(
        out.stderr.to_lowercase().contains("cannot be used with")
            || out.stderr.to_lowercase().contains("conflict"),
        "stderr: {}",
        out.stderr
    );
}

#[test]
fn hostname_fqdn_and_domain() {
    let fqdn = cash("hostname -f");
    assert_eq!(fqdn.code, 0);
    assert!(!fqdn.stdout.is_empty());

    let domain = cash("hostname -d");
    assert_eq!(domain.code, 0);
}

// ===========================================================================
// 2. logout parameter and environment tests
// ===========================================================================

#[test]
fn logout_in_login_shell_with_custom_exit_code() {
    let out = cash_login("logout 42");
    assert_eq!(out.code, 42, "logout 42 should exit with 42");
}

#[test]
fn logout_in_login_shell_default_code() {
    let out = cash_login("true; logout");
    assert_eq!(out.code, 0);

    let out_err = cash_login("false; logout");
    assert_eq!(out_err.code, 1);
}

#[test]
fn logout_invalid_numeric_argument_fails() {
    let out = cash_login("logout notanumber");
    assert_ne!(out.code, 0);
}

#[test]
fn logout_in_non_login_shell_errors() {
    let out = cash("logout");
    assert_eq!(out.code, 1);
    assert!(
        out.stderr.contains("not login shell"),
        "stderr: {}",
        out.stderr
    );
}

// ===========================================================================
// 3. which parameter tests
// ===========================================================================

#[test]
fn which_missing_operand_fails_with_usage() {
    let out = cash("which");
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("usage"), "stderr: {}", out.stderr);
}

#[test]
fn which_finds_builtins_by_default() {
    let out = cash("which cd");
    assert_eq!(out.code, 0);
    assert_eq!(out.stdout, "cd: shell builtin");
}

#[test]
fn which_path_only_flag_skips_builtins() {
    let out = cash("which -p cd");
    assert_ne!(out.code, 0, "cd is not an executable on path");
    assert!(out.stdout.is_empty());
}

#[test]
fn which_silent_flag_reports_only_exit_code() {
    let success = cash("which -s echo");
    assert_eq!(success.code, 0);
    assert!(success.stdout.is_empty());
    assert!(success.stderr.is_empty());

    let failure = cash("which -s non_existent_command_xyz_12345");
    assert_ne!(failure.code, 0);
    assert!(failure.stdout.is_empty());
    assert!(failure.stderr.is_empty());
}

#[test]
fn which_multiple_names_exits_with_error_if_any_missing() {
    let out = cash("which echo non_existent_command_xyz_12345");
    assert_ne!(out.code, 0);
    assert!(out.stdout.contains("builtin"));
    assert!(out.stderr.contains("not found"));
}

// ===========================================================================
// 4. type parameter tests
// ===========================================================================

#[test]
fn type_type_only_flag_reports_exact_kind() {
    assert_eq!(cash("type -t echo").stdout, "builtin");
    assert_eq!(cash("type -t if").stdout, "keyword");

    let out_fn = cash("myfunc() { :; }; type -t myfunc");
    assert_eq!(out_fn.stdout, "function");

    let out_alias = cash("shopt -s expand_aliases; alias myalias=ls; type -t myalias");
    assert_eq!(out_alias.stdout, "alias");
}

#[test]
fn type_suppress_function_lookup() {
    let out = cash("cd() { :; }; type -f cd");
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("builtin"));
    assert!(!out.stdout.contains("function"));
}

#[test]
fn type_nonexistent_command_returns_nonzero() {
    let out = cash("type nonexistent_command_foo_bar");
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("not found"));
}

// ===========================================================================
// 5. id and groups parameter tests
// ===========================================================================

#[test]
fn id_flags_individual_and_combined() {
    let id_u = cash("id -u");
    assert_eq!(id_u.code, 0);
    assert!(id_u.stdout.parse::<u64>().is_ok());

    let id_un = cash("id -un");
    assert_eq!(id_un.code, 0);
    assert!(!id_un.stdout.is_empty());

    let id_g = cash("id -g");
    assert_eq!(id_g.code, 0);

    let id_gn = cash("id -gn");
    assert_eq!(id_gn.code, 0);

    let id_groups = cash("id -G");
    assert_eq!(id_groups.code, 0);

    let id_groups_name = cash("id -Gn");
    assert_eq!(id_groups_name.code, 0);
}

#[test]
fn groups_reports_current_user_groups() {
    let out = cash("groups");
    assert_eq!(out.code, 0);
    assert!(!out.stdout.is_empty());
}

// ===========================================================================
// 6. find compound logic, -empty, -size, and -exec
// ===========================================================================

#[test]
fn find_boolean_compound_operators() {
    let sb = Sandbox::new("find-compound");
    std::fs::write(sb.root.join("alpha.txt"), "hello").unwrap();
    std::fs::write(sb.root.join("beta.md"), "world").unwrap();
    std::fs::write(sb.root.join("gamma.txt"), "foo").unwrap();

    // -and / -a
    let out_and = sb.run(r#"find . -name "*.txt" -and -name "a*""#);
    assert!(out_and.stdout.contains("alpha.txt"));
    assert!(!out_and.stdout.contains("gamma.txt"));

    // -or / -o
    let out_or = sb.run(r#"find . -name "*.txt" -or -name "*.md""#);
    assert!(out_or.stdout.contains("alpha.txt"));
    assert!(out_or.stdout.contains("beta.md"));
    assert!(out_or.stdout.contains("gamma.txt"));

    // -not / !
    let out_not = sb.run(r#"find . -type f -not -name "*.txt""#);
    assert!(!out_not.stdout.contains("alpha.txt"));
    assert!(out_not.stdout.contains("beta.md"));
}

#[test]
fn find_empty_and_size_units() {
    let sb = Sandbox::new("find-empty-size");
    std::fs::write(sb.root.join("empty.txt"), "").unwrap();
    std::fs::write(sb.root.join("small.txt"), "12345").unwrap();
    std::fs::create_dir(sb.root.join("emptydir")).unwrap();

    // -empty on files
    let out_empty_f = sb.run("find . -type f -empty");
    assert!(out_empty_f.stdout.contains("empty.txt"));
    assert!(!out_empty_f.stdout.contains("small.txt"));

    // -empty on directories
    let out_empty_d = sb.run("find . -type d -empty");
    assert!(out_empty_d.stdout.contains("emptydir"));

    // -size with exact byte count 'c'
    let out_size_5c = sb.run("find . -type f -size 5c");
    assert!(out_size_5c.stdout.contains("small.txt"));
    assert!(!out_size_5c.stdout.contains("empty.txt"));
}

#[test]
fn find_depth_limits() {
    let sb = Sandbox::new("find-depth");
    let deep = sb.root.join("a").join("b").join("c");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("deep.txt"), "content").unwrap();

    let out_max0 = sb.run("find . -maxdepth 0");
    assert_eq!(out_max0.stdout, ".");

    let out_max1 = sb.run("find . -maxdepth 1 -name 'deep.txt'");
    assert!(out_max1.stdout.is_empty());
}

#[test]
fn find_exec_single_and_batched() {
    let sb = Sandbox::new("find-exec");
    std::fs::write(sb.root.join("1.log"), "a").unwrap();
    std::fs::write(sb.root.join("2.log"), "b").unwrap();

    // -exec ... \; runs per-match
    let out_single = sb.run(r#"find . -type f -name "*.log" -exec echo ITEM: {} \;"#);
    assert_eq!(out_single.stdout.matches("ITEM:").count(), 2);

    // -exec ... + batches arguments
    let out_batch = sb.run(r#"find . -type f -name "*.log" -exec echo BATCH: {} +"#);
    assert_eq!(out_batch.stdout.matches("BATCH:").count(), 1);
    assert!(out_batch.stdout.contains("1.log"));
    assert!(out_batch.stdout.contains("2.log"));
}

// ===========================================================================
// 7. xargs parameter tests
// ===========================================================================

#[test]
fn xargs_max_args_limits_batch_size() {
    let out = cash("printf '1 2 3 4 5' | xargs -n 2 echo GROUP:");
    let count = out.stdout.matches("GROUP:").count();
    assert_eq!(
        count, 3,
        "expected 3 batches of at most 2 args: {}",
        out.stdout
    );
}

#[test]
fn xargs_no_run_if_empty_flag() {
    let without_r = cash("printf '' | xargs echo RAN");
    assert_eq!(without_r.stdout, "RAN");

    let with_r = cash("printf '' | xargs -r echo RAN");
    assert!(with_r.stdout.is_empty());
}

#[test]
fn xargs_null_separated_preserves_spaces_and_newlines() {
    let out = cash(r#"printf 'hello world\0line\nwith\nnewlines\0' | xargs -0 -n 1 echo ENTRY:"#);
    assert_eq!(out.code, 0, "stderr: {}", out.stderr);
    assert_eq!(
        out.stdout,
        "ENTRY: hello world\nENTRY: line\nwith\nnewlines"
    );
}

#[test]
fn xargs_builtin_failure_and_control_flow_are_isolated() {
    assert_eq!(cash("printf 'item' | xargs false").code, 123);
    let out = cash("printf '' | xargs exit 7; echo survived");
    assert_eq!(out.stdout, "survived");
    assert_eq!(out.code, 0);
}

#[test]
fn xargs_prefers_builtins_over_path_for_default_and_explicit_echo() {
    let sandbox = Sandbox::new("xargs-shadow-echo");
    // cash renamed to echo.exe is deliberately not an echo implementation. This
    // catches accidental PATH lookup on CI without requiring Git or Scoop tools.
    let external = sandbox.root.join("echo.exe");
    std::fs::copy(CASH, &external).unwrap();
    for script in ["printf 'hello' | xargs", "printf 'hello' | xargs echo"] {
        let out = cash_command()
            .env("PATH", sandbox.root.path())
            .args(["-c", script])
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        assert_eq!(out.stdout, b"hello\n");
    }

    // Disabling the builtin must restore external lookup, without falling back
    // to the bundled utility. The renamed cash understands this explicit mode.
    let out = cash_command()
        .env("PATH", sandbox.root.path())
        .args([
            "-c",
            "enable -n echo; printf 'hello' | xargs echo --invoke-bundled echo EXTERNAL",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    assert_eq!(out.stdout, b"EXTERNAL hello\n");

    let executable = external
        .to_string_lossy()
        .replace('\\', "/")
        .replace('\'', "'\\''");
    let out = cash(&format!(
        "printf 'hello' | xargs '{executable}' --invoke-bundled echo EXPLICIT"
    ));
    assert_eq!(out.code, 0, "{}", out.stderr);
    assert_eq!(out.stdout, "EXPLICIT hello");
}

#[test]
fn xargs_replace_token() {
    let out = cash("printf 'apple\nbanana\n' | xargs -I {} echo 'fruit: {}'");
    assert!(out.stdout.contains("fruit: apple"));
    assert!(out.stdout.contains("fruit: banana"));
}

// ===========================================================================
// 8. mapfile / readarray parameter tests
// ===========================================================================

#[test]
fn mapfile_strips_delimiters_with_t() {
    let out = cash(
        r#"
        mapfile -t lines <<'EOF'
first line
second line
EOF
        echo "count=${#lines[@]} 0=[${lines[0]}] 1=[${lines[1]}]"
        "#,
    );
    assert_eq!(out.stdout, "count=2 0=[first line] 1=[second line]");
}

#[test]
fn mapfile_t_takes_a_crlf_ending_whole() {
    // D20: `\r\n` ends a line as `\n` does; `-t` left the `\r` (XC-4). Without `-t` the
    // ending is kept as it came, and a `\r` inside a line is the line's.
    let out = cash(
        r#"printf 'a\r\nb\r\n' | { mapfile -t t; printf '[%q]' "${t[@]}"; echo; }; printf 'a\r\n' | { mapfile k; printf '[%q]' "${k[@]}"; echo; }; printf 'p\rq\r\n' | { mapfile -t w; printf '[%q]' "${w[@]}"; }"#,
    );
    assert_eq!(out.stdout, "[a][b]\n[$'a\\r\\n']\n[$'p\\rq']");
}

#[test]
fn mapfile_max_count_and_skip() {
    let out = cash(
        r#"
        printf 'a\nb\nc\nd\ne\n' | {
            mapfile -s 1 -n 2 -t arr
            echo "${arr[*]}"
        }
        "#,
    );
    assert_eq!(out.stdout, "b c");
}

#[test]
fn mapfile_origin_offset() {
    let out = cash(
        r#"
        printf 'two\nthree\n' | {
            arr=(zero one)
            mapfile -O 2 -t arr
            echo "${arr[0]} ${arr[1]} ${arr[2]} ${arr[3]}"
        }
        "#,
    );
    assert_eq!(out.stdout, "zero one two three");
}

// ===========================================================================
// 9. read parameter tests
// ===========================================================================

#[test]
fn read_array_variable_populates_words() {
    let out = cash(
        r#"
        read -a words <<< "alpha beta gamma delta"
        echo "${#words[@]} : ${words[1]} : ${words[3]}"
        "#,
    );
    assert_eq!(out.stdout, "4 : beta : delta");
}

#[test]
fn read_custom_delimiter() {
    let out = cash(
        r#"
        read -d ':' token <<< "segment1:segment2"
        echo "$token"
        "#,
    );
    assert_eq!(out.stdout, "segment1");
}

#[test]
fn read_character_limits_n_and_capital_n() {
    let out_n = cash(r#"read -n 3 str <<< "abcdef"; echo "$str""#);
    assert_eq!(out_n.stdout, "abc");

    let out_cap_n = cash(r#"read -N 4 str <<< "abcdef"; echo "$str""#);
    assert_eq!(out_cap_n.stdout, "abcd");
}

// ===========================================================================
// 10. getopts option parsing
// ===========================================================================

#[test]
fn getopts_parses_flags_and_arguments() {
    let out = cash(
        r#"
        parse() {
            local OPTIND opt
            while getopts "a:b" opt; do
                case "$opt" in
                    a) echo "A:$OPTARG" ;;
                    b) echo "B" ;;
                    ?) echo "ERR" ;;
                esac
            done
            shift "$((OPTIND-1))"
            echo "REST:$*"
        }
        parse -b -a myval positional
        "#,
    );
    assert!(out.stdout.contains('B'));
    assert!(out.stdout.contains("A:myval"));
    assert!(out.stdout.contains("REST:positional"));
}

#[test]
fn getopts_silent_mode_on_missing_arg() {
    let out = cash(
        r#"
        parse() {
            local OPTIND opt
            while getopts ":a:" opt; do
                case "$opt" in
                    :) echo "MISSING:$OPTARG" ;;
                    \?) echo "UNKNOWN:$OPTARG" ;;
                esac
            done
        }
        parse -a
        "#,
    );
    assert_eq!(out.stdout, "MISSING:a");
}

// ===========================================================================
// 11. let arithmetic and exit code contract
// ===========================================================================

#[test]
fn let_returns_success_on_nonzero_result() {
    let out = cash("let a=10+5; echo $?");
    assert_eq!(out.stdout, "0");

    let out_val = cash("let a=10+5; echo $a");
    assert_eq!(out_val.stdout, "15");
}

#[test]
fn let_returns_exit_1_on_zero_result() {
    // Standard bash rule: let returns 1 if last expression evaluates to 0
    let out = cash("let 1-1; echo $?");
    assert_eq!(out.stdout, "1");
}

// ===========================================================================
// 12. hash command parameters
// ===========================================================================

#[test]
fn hash_reset_and_manual_path() {
    let out = cash(
        r#"
        hash -r
        hash -p /c/Windows/System32/cmd.exe mycmd
        hash -t mycmd
        "#,
    );
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("cmd.exe"));
}

#[test]
fn hash_delete_entry() {
    let out = cash(
        r#"
        hash -p /c/Windows/System32/cmd.exe mycmd
        hash -d mycmd
        hash -t mycmd 2>/dev/null || echo removed
        "#,
    );
    assert!(out.stdout.contains("removed"));
}

// ===========================================================================
// 13. help command options
// ===========================================================================

#[test]
fn help_short_description_and_usage() {
    let desc = cash("help -d cd");
    assert_eq!(desc.code, 0);
    assert!(desc.stdout.contains("cd -"));

    let usage = cash("help -s cd");
    assert_eq!(usage.code, 0);
    assert!(usage.stdout.contains("cd: cd"));
}

#[test]
fn help_wildcard_topic_matching() {
    let out = cash("help 'c*'");
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("cd"));
}

#[test]
fn help_nonexistent_topic_exits_with_error() {
    let out = cash("help nonexistent_topic_xyz_123");
    assert_ne!(out.code, 0);
}

// ===========================================================================
// 14. enable command parameters
// ===========================================================================

#[test]
fn enable_disable_and_reenable() {
    let out = cash(
        r#"
        enable -n cd
        enable -p | grep "enable -n cd"
        enable cd
        enable -p | grep "enable cd"
        "#,
    );
    assert_eq!(out.code, 0);
}

#[test]
fn enable_nonexistent_builtin_fails() {
    let out = cash("enable nonexistent_builtin_xyz");
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("not a shell builtin"));
}

// ===========================================================================
// 15. chmod parameter tests
// ===========================================================================

#[test]
fn chmod_modify_readonly_attribute() {
    let sb = Sandbox::new("chmod-test");
    let file = sb.root.join("writable.txt");
    std::fs::write(&file, "data").unwrap();

    // Make read-only
    let out_ro = sb.run("chmod -w writable.txt; [ -w writable.txt ] && echo W || echo RO");
    assert_eq!(out_ro.stdout, "RO");

    // Make writable
    let out_w = sb.run("chmod +w writable.txt; [ -w writable.txt ] && echo W || echo RO");
    assert_eq!(out_w.stdout, "W");
}

#[test]
fn chmod_execute_warning_and_silence_flag() {
    let sb = Sandbox::new("chmod-warn");
    std::fs::write(sb.root.join("script.sh"), "#!/bin/sh\n").unwrap();

    let out_warn = sb.run("chmod -x script.sh");
    assert!(
        out_warn.stderr.contains("not represented"),
        "expected warning on stderr: {}",
        out_warn.stderr
    );

    let out_silent = sb.run("chmod -f -x script.sh");
    assert!(out_silent.stderr.is_empty());
}

// ===========================================================================
// 16. top parameter tests
// ===========================================================================

#[test]
fn top_sorting_options() {
    let out_cpu = cash("top -b -n 1 -d 0.1 -o cpu");
    assert_eq!(out_cpu.code, 0);

    let out_mem = cash("top -b -n 1 -d 0.1 -o mem");
    assert_eq!(out_mem.code, 0);
}

#[test]
fn top_invalid_sort_field_fails() {
    let out = cash("top -b -n 1 -d 0.1 -o nonexistent_field");
    assert_ne!(out.code, 0);
    assert!(out.stderr.contains("unknown sort field"));
}

// ===========================================================================
// 17. builtin, command, and alias isolation
// ===========================================================================

#[test]
fn builtin_keyword_bypasses_function_override() {
    let out = cash(
        r#"
        echo() { printf 'FUNC:%s\n' "$*"; }
        echo test
        builtin echo test
        "#,
    );
    assert!(out.stdout.contains("FUNC:test"));
    assert!(out.stdout.contains("test"));
}

#[test]
fn command_v_and_capital_v() {
    let out_v = cash("command -v cd");
    assert_eq!(out_v.stdout, "cd");

    let out_cap_v = cash("command -V cd");
    assert!(out_cap_v.stdout.contains("builtin"));
}

#[test]
fn alias_definition_and_unalias_all() {
    let out = cash(
        r#"
        shopt -s expand_aliases
        alias greet='echo hello'
        greet
        unalias greet
        alias greet 2>/dev/null || echo UNALIASED
        "#,
    );
    assert!(out.stdout.contains("hello"));
    assert!(out.stdout.contains("UNALIASED"));
}

// ===========================================================================
// 18. declare, readonly, and local
// ===========================================================================

#[test]
fn declare_integer_attribute() {
    let out = cash(
        r#"
        declare -i val=25
        echo "$val"
        val+=10
        echo "$val"
        "#,
    );
    assert_eq!(out.stdout, "25\n35");
}

#[test]
fn readonly_prevents_mutation() {
    let out = cash(
        r#"
        readonly my_const="immutable"
        ( my_const="mutated" ) 2>/dev/null
        echo "protected: $my_const"
        "#,
    );
    assert_eq!(out.stdout, "protected: immutable");
}

// ===========================================================================
// 19. printf and echo formatting specifiers
// ===========================================================================

#[test]
fn printf_hex_and_padded_numbers() {
    let out = cash(r#"printf '%05d %x\n' 42 255"#);
    assert_eq!(out.stdout, "00042 ff");
}

#[test]
fn printf_says_what_is_not_a_number_and_fails() {
    // uucore said it straight to the process's standard error, past `2>/dev/null` and
    // `$(…)`, and the status was 0 (BI-09).
    let out = cash(
        r#"printf '%d\n' abc; echo "rc $?"; printf '%d\n' 2x 2>/dev/null; echo "rc $?"; e=$(printf '%d' q 2>&1); echo "[$e]""#,
    );
    // As Bash says them, after `script: line 1: `.
    assert!(
        out.stdout.starts_with("0\nrc 1\n2\nrc 1\n[")
            && out
                .stdout
                .ends_with(": line 1: printf: q: invalid number\n0]"),
        "{}",
        out.stdout
    );
    assert!(
        out.stderr
            .ends_with(": line 1: printf: abc: invalid number")
            && out.stderr.lines().count() == 1,
        "{}",
        out.stderr
    );
}

#[test]
fn printf_stops_all_output_at_backslash_c() {
    // `\c` in `%b` ended only that pass; the arguments left over were formatted (BI-10).
    let out = cash(r#"printf '%b|' 'a\cb' c d; echo; printf '%s %b\n' x 'y\cz' w v"#);
    assert_eq!(out.stdout, "a\nx y");
}

#[test]
fn echo_n_and_e_escape_processing() {
    let out_n = cash(r#"echo -n "no-newline""#);
    assert_eq!(out_n.stdout, "no-newline");

    let out_e = cash(r#"echo -e "line1\nline2""#);
    assert_eq!(out_e.stdout, "line1\nline2");
}

// ===========================================================================
// 20. test and [ conditional evaluations
// ===========================================================================

#[test]
fn test_and_bracket_integer_comparisons() {
    assert_eq!(cash("[ 10 -gt 5 ] && echo YES || echo NO").stdout, "YES");
    assert_eq!(cash("[ 5 -gt 10 ] && echo YES || echo NO").stdout, "NO");
    assert_eq!(cash("[ 10 -eq 10 ] && echo YES || echo NO").stdout, "YES");
    assert_eq!(cash("[ 10 -ne 5 ] && echo YES || echo NO").stdout, "YES");
    assert_eq!(cash("[ 5 -le 5 ] && echo YES || echo NO").stdout, "YES");
    assert_eq!(cash("[ 4 -lt 5 ] && echo YES || echo NO").stdout, "YES");
}

#[test]
fn test_and_bracket_string_and_file_tests() {
    assert_eq!(cash(r#"[ -z "" ] && echo EMPTY"#).stdout, "EMPTY");
    assert_eq!(cash(r#"[ -n "nonempty" ] && echo OK"#).stdout, "OK");
    assert_eq!(cash(r#"[ "abc" = "abc" ] && echo EQ"#).stdout, "EQ");
    assert_eq!(cash(r#"[ "abc" != "xyz" ] && echo NE"#).stdout, "NE");

    let sb = Sandbox::new("bracket-file-tests");
    let file = sb.root.join("sample.txt");
    std::fs::write(&file, "contents").unwrap();

    assert_eq!(
        sb.run("[ -f sample.txt ] && echo IS_FILE").stdout,
        "IS_FILE"
    );
    assert_eq!(sb.run("[ -d . ] && echo IS_DIR").stdout, "IS_DIR");
    assert_eq!(
        sb.run("[ -s sample.txt ] && echo HAS_SIZE").stdout,
        "HAS_SIZE"
    );
    assert_eq!(
        sb.run("[ ! -e non_existent_file ] && echo NOT_FOUND")
            .stdout,
        "NOT_FOUND"
    );
}

// ===========================================================================
// 21. shopt shell options toggling
// ===========================================================================

#[test]
fn shopt_set_unset_and_query() {
    let out = cash(
        r#"
        shopt -s extglob
        shopt -q extglob && echo ON
        shopt -u extglob
        shopt -q extglob || echo OFF
        "#,
    );
    assert_eq!(out.stdout, "ON\nOFF");
}

// ===========================================================================
// 22. set, shift, and positional parameters
// ===========================================================================

#[test]
fn set_positional_parameters_and_shift() {
    let out = cash(
        r#"
        set -- apple banana cherry date
        echo "count=$# 1=$1 2=$2"
        shift 2
        echo "after_shift: count=$# 1=$1 2=$2"
        "#,
    );
    assert_eq!(
        out.stdout,
        "count=4 1=apple 2=banana\nafter_shift: count=2 1=cherry 2=date"
    );
}

// ===========================================================================
// 23. dirs, pushd, and popd directory stack
// ===========================================================================

#[test]
fn directory_stack_pushd_popd_and_dirs() {
    let sb = Sandbox::new("dirstack");
    let sub = sb.root.join("sub");
    std::fs::create_dir_all(&sub).unwrap();

    let out = sb.run(
        r#"
        pushd sub > /dev/null
        pwd_sub=$(pwd)
        popd > /dev/null
        pwd_root=$(pwd)
        [ "$pwd_sub" != "$pwd_root" ] && echo STACK_WORKS
        "#,
    );
    assert_eq!(out.stdout, "STACK_WORKS");
}

// ===========================================================================
// 24. cd, pwd, and OLDPWD
// ===========================================================================

#[test]
fn cd_hyphen_toggles_previous_directory() {
    let sb = Sandbox::new("cd-tests");
    let dir1 = sb.root.join("dir1");
    let dir2 = sb.root.join("dir2");
    std::fs::create_dir_all(&dir1).unwrap();
    std::fs::create_dir_all(&dir2).unwrap();

    let out = sb.run(
        r#"
        cd dir1
        p1=$(pwd)
        cd ../dir2
        p2=$(pwd)
        cd - > /dev/null
        p_back=$(pwd)
        [ "$p1" = "$p_back" ] && echo TOGGLE_SUCCESS
        "#,
    );
    assert_eq!(out.stdout, "TOGGLE_SUCCESS");
}

// ===========================================================================
// 25. eval dynamic execution
// ===========================================================================

#[test]
fn eval_dynamically_evaluates_constructed_commands() {
    let out = cash(
        r#"
        cmd="var=dynamic_value"
        eval "$cmd"
        echo "$var"
        "#,
    );
    assert_eq!(out.stdout, "dynamic_value");
}

// ===========================================================================
// 26. times reports user and system execution time
// ===========================================================================

#[test]
fn times_reports_execution_time() {
    let out = cash("times");
    assert_eq!(out.code, 0);
    let lines: Vec<&str> = out.stdout.lines().collect();
    assert_eq!(
        lines.len(),
        2,
        "times should report 2 lines (shell and children)"
    );
    assert!(lines[0].contains('m') && lines[0].contains('s'));
    assert!(lines[1].contains('m') && lines[1].contains('s'));
}

#[test]
fn time_and_times_count_cpu_time() {
    // Both always said 0 (EXE-11): busy work in the shell is its own user time, and in a
    // child (`bash` is cash, D7) the children's, which `times` reports on its second line.
    let out = cash(
        r#"
        TIMEFORMAT='%3U'
        time { i=0; while ((i < 20000)); do ((i++)); done; }
        time bash -c 'i=0; while ((i < 20000)); do ((i++)); done'
        times
        "#,
    );
    let seconds = |text: &str| -> f64 { text.trim().parse().expect("a number of seconds") };
    let reports: Vec<&str> = out.stderr.lines().collect();
    assert_eq!(reports.len(), 2, "{}", out.stderr);
    assert!(seconds(reports[0]) > 0.0, "shell: {}", out.stderr);
    assert!(seconds(reports[1]) > 0.0, "child: {}", out.stderr);

    let children = out.stdout.lines().nth(1).expect("times' second line");
    assert_ne!(
        children.split_whitespace().next(),
        Some("0m0.000s"),
        "{children}"
    );
}

// ===========================================================================
// 27. trap EXIT execution
// ===========================================================================

#[test]
fn trap_exit_executes_on_normal_exit() {
    let out = cash(
        r#"
        trap 'echo TRAPPED_EXIT' EXIT
        echo MAIN_FLOW
        "#,
    );
    assert_eq!(out.stdout, "MAIN_FLOW\nTRAPPED_EXIT");
}

// ===========================================================================
// 28. exit code propagation
// ===========================================================================

#[test]
fn exit_builtin_propagates_exact_numeric_code() {
    assert_eq!(cash("exit 0").code, 0);
    assert_eq!(cash("exit 42").code, 42);
    assert_eq!(cash("exit 125").code, 125);
}

// ===========================================================================
// 29. break and continue loop control
// ===========================================================================

#[test]
fn break_terminates_loop() {
    let out = cash(
        r#"
        for i in 1 2 3 4; do
            if [ "$i" -eq 3 ]; then break; fi
            echo -n "$i "
        done
        "#,
    );
    assert_eq!(out.stdout, "1 2");
}

#[test]
fn continue_skips_iteration() {
    let out = cash(
        r#"
        for i in 1 2 3 4; do
            if [ "$i" -eq 2 ]; then continue; fi
            echo -n "$i "
        done
        "#,
    );
    assert_eq!(out.stdout, "1 3 4");
}

// ===========================================================================
// 30. dot (.) and source
// ===========================================================================

#[test]
fn dot_and_source_execute_in_current_environment() {
    let sb = Sandbox::new("dot-source");
    let script = sb.root.join("env_setup.sh");
    std::fs::write(&script, "SOURCED_VAL=\"active\"\nPARAM_ONE=\"$1\"\n").unwrap();

    let out_dot = sb.run(". ./env_setup.sh arg1; echo \"val=$SOURCED_VAL param=$PARAM_ONE\"");
    assert_eq!(out_dot.stdout, "val=active param=arg1");

    let out_source =
        sb.run("source ./env_setup.sh arg2; echo \"val=$SOURCED_VAL param=$PARAM_ONE\"");
    assert_eq!(out_source.stdout, "val=active param=arg2");
}

// ===========================================================================
// 31. return and local inside functions
// ===========================================================================

#[test]
fn function_return_code_and_local_scoping() {
    let out = cash(
        r#"
        outer="global"
        test_fn() {
            local outer="inner"
            return 37
        }
        test_fn
        code=$?
        echo "code=$code outer=$outer"
        "#,
    );
    assert_eq!(out.stdout, "code=37 outer=global");
}

// ===========================================================================
// 32. unset variables and functions
// ===========================================================================

#[test]
fn unset_variables_and_functions() {
    let out_var = cash(
        r#"
        my_var="present"
        unset my_var
        echo "var=[${my_var:-EMPTY}]"
        "#,
    );
    assert_eq!(out_var.stdout, "var=[EMPTY]");

    let out_func = cash(
        r#"
        my_fn() { echo RUN; }
        unset -f my_fn
        type my_fn 2>/dev/null || echo "UNSET_SUCCESS"
        "#,
    );
    assert_eq!(out_func.stdout, "UNSET_SUCCESS");
}

// ===========================================================================
// 33. export and typeset
// ===========================================================================

#[test]
fn export_passes_variable_to_subprocesses() {
    let out = cash(
        r#"
        export TEST_EXPORT_VAR="visible"
        cash -c 'echo "$TEST_EXPORT_VAR"'
        "#,
    );
    assert_eq!(out.stdout, "visible");
}

#[test]
fn typeset_behaves_as_declare() {
    let out = cash(
        r#"
        typeset -r my_const="constant"
        ( my_const="changed" ) 2>/dev/null
        echo "$my_const"
        "#,
    );
    assert_eq!(out.stdout, "constant");
}

// ===========================================================================
// 34. ulimit on Windows
// ===========================================================================

#[test]
fn ulimit_reports_unlimited_and_accepts_values() {
    let out_bare = cash("ulimit");
    assert_eq!(out_bare.stdout, "unlimited");

    let out_all = cash("ulimit -a");
    assert!(out_all.stdout.contains("open files"));
    assert!(out_all.stdout.contains("unlimited"));

    // Setting a limit is silently accepted without breaking set -e scripts
    let out_set = cash("set -e; ulimit -n 2048; echo SUCCESS");
    assert_eq!(out_set.stdout, "SUCCESS");

    // Bash 5.2 assigns an operand left after option parsing to the final resource
    // option, and ignores later operands. Windows validates the same command shapes
    // even though there is no Win32 rlimit to apply.
    let out_trailing = cash("ulimit -n -S 2048 extra; echo status=$?");
    assert_eq!(out_trailing.stdout, "status=0");

    let out_invalid = cash("ulimit -n not-a-limit; echo status=$?");
    assert_eq!(out_invalid.stdout, "status=2");
}

// ===========================================================================
// 35. umask on Windows
// ===========================================================================

#[test]
fn umask_octal_and_symbolic_reporting() {
    let out_default = cash("umask");
    assert_eq!(out_default.stdout, "0022");

    let out_sym = cash("umask -S");
    assert_eq!(out_sym.stdout, "u=rwx,g=rx,o=rx");

    let out_p = cash("umask -p");
    assert_eq!(out_p.stdout, "umask 0022");

    let out_change = cash("umask 0077; umask");
    assert_eq!(out_change.stdout, "0077");
}

// ===========================================================================
// 36. wait, jobs, and kill -l
// ===========================================================================

#[test]
fn kill_l_lists_signals() {
    let out = cash("kill -l");
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("KILL"));
    assert!(out.stdout.contains("TERM"));
}

#[test]
fn wait_reaps_background_job() {
    let out = cash(
        r#"
        sleep 0.1 &
        pid=$!
        wait "$pid"
        echo "waited: $?"
        "#,
    );
    assert_eq!(out.stdout, "waited: 0");
}

// ===========================================================================
// 37. complete and compgen
// ===========================================================================

#[test]
fn compgen_wordlist_filtering() {
    let out = cash(r#"compgen -W "alpha beta apple apricot" a"#);
    assert_eq!(out.stdout, "alpha\napple\napricot");
}

#[test]
fn complete_registration_and_query() {
    let out = cash(
        r#"
        complete -W "start stop restart" myservice
        complete -p myservice
        "#,
    );
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("myservice"));
    assert!(out.stdout.contains("start stop restart"));
}

// ===========================================================================
// 38. bind readline inspection
// ===========================================================================

#[test]
fn bind_lists_readline_functions_and_variables() {
    // In non-interactive mode, bind silently succeeds with code 0
    let out_funcs = cash("bind -l");
    assert_eq!(out_funcs.code, 0);

    let out_vars = cash("bind -V");
    assert_eq!(out_vars.code, 0);
}

// ===========================================================================
// 39. history built-in
// ===========================================================================

#[test]
fn history_inspection_and_clear() {
    // History is enabled in interactive shells (-i)
    let out = run(&["--norc", "-i", "-c", "history -c; history"]);
    assert_eq!(out.code, 0);
}

// ===========================================================================
// 40. readarray (mapfile alias)
// ===========================================================================

#[test]
fn readarray_populates_array_from_stdin() {
    let out = cash(
        r#"
        readarray -t myarr <<'EOF'
first
second
EOF
        echo "${#myarr[@]}: ${myarr[0]} & ${myarr[1]}"
        "#,
    );
    assert_eq!(out.stdout, "2: first & second");
}

// ===========================================================================
// 41. colon (:), true, and false
// ===========================================================================

#[test]
fn colon_true_and_false_exit_statuses() {
    assert_eq!(cash(":").code, 0);
    assert_eq!(cash("true").code, 0);
    assert_eq!(cash("false").code, 1);
}

// ===========================================================================
// 42. detach Windows built-in
// ===========================================================================

#[test]
fn detach_starts_background_process() {
    let out = cash("detach cmd.exe /c exit 0");
    assert_eq!(out.code, 0);
    assert!(out.stdout.contains("[detached] pid"));
}
