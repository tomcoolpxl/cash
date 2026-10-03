//! Environment variable retrieval for Windows.
//!
//! On Windows, well-known environment variable names are normalized to their
//! canonical POSIX forms (e.g. `Path` → `PATH`), and `HOME` is synthesized
//! from `USERPROFILE` or `HOMEDRIVE`+`HOMEPATH` if not already present.

use super::render_canonical_path;
use std::collections::BTreeMap;

/// Retrieves environment variables from the host process, applying
/// Windows-specific fixups.
///
/// Normalizes well-known variable names to POSIX conventions, copies
/// `TEMP`/`TMP` to `TMPDIR` (preserving originals for native Windows apps),
/// and synthesizes `HOME` if it is not natively defined.
///
/// A [`BTreeMap`] is used (rather than `HashMap`) so iteration order is
/// deterministic across runs. If two source variables collide under the same
/// canonical name (e.g. both `Path` and `PATH` are set to different values),
/// the conflict is logged and the last-seen value wins.
pub(crate) fn get_host_env_vars() -> impl Iterator<Item = (String, String)> {
    collect_host_env_vars(std::env::vars())
}

/// Collects and normalizes a set of environment variables. Exposed as a
/// pure function (taking the source iterator) so it can be unit-tested
/// without touching the process environment.
fn collect_host_env_vars<I>(source: I) -> std::collections::btree_map::IntoIter<String, String>
where
    I: IntoIterator<Item = (String, String)>,
{
    let mut vars = BTreeMap::new();

    // Normalize host env vars and inject them into the map.
    for (k, v) in source {
        let normalized = normalize_env_name(&k);
        if let Some(existing) = vars.get(&normalized)
            && existing != &v
        {
            tracing::warn!(
                "environment variable collision under canonical name {normalized}: \
                 two different values were supplied (last-write wins)"
            );
        }
        vars.insert(normalized, v);
    }

    // Synthesize HOME from Windows-native variables if not already present.
    if !vars.contains_key("HOME") {
        let home = vars.get("USERPROFILE").cloned().or_else(|| {
            let d = vars.get("HOMEDRIVE")?;
            let p = vars.get("HOMEPATH")?;
            Some(format!("{d}{p}"))
        });
        if let Some(home) = home {
            // cash (D3): render canonically. $HOME is the root of ~ expansion,
            // $HISTFILE and the .cashrc path, so a backslash spelling here leaks into
            // all of them and leaves $PWD and $HOME disagreeing.
            vars.insert("HOME".to_string(), home);
        }
    }

    // cash (D3): render HOME canonically whether it was synthesized above or inherited
    // from the Windows environment, which is the common case. Otherwise `$PWD` reads
    // `C:/Users/me` while `$HOME` reads `C:\Users\me`, and string comparisons
    // between them silently fail.
    if let Some(home) = vars.get("HOME").cloned() {
        vars.insert("HOME".to_string(), render_canonical_path(&home));
    }

    // cash (D5): PATH is rendered Unix-style to scripts — colon-separated, `/c/...`.
    //
    // D5 is explicit that this is the one variable cash has semantic knowledge of, and
    // that scripts see the Unix form so `IFS=: read -ra dirs <<< "$PATH"` works and
    // `PATH=/foo:$PATH` composes. It was never implemented: `echo $PATH` returned the
    // raw Windows value, semicolons and backslashes, so both of those idioms produced
    // nonsense. The Windows form is rebuilt in the environment block handed to a child,
    // which is where `terraform.exe` and `git.exe` need it.
    if let Some(path) = vars.get("PATH").cloned() {
        vars.insert("PATH".to_string(), cash_win32::env::path_to_unix(&path));
    }

    // cash: `$SHELL` names *this* shell.
    //
    // It is inherited from whatever launched cash — Git Bash, PowerShell, Windows
    // Terminal — so it pointed at another shell entirely. Anything that reads it to
    // decide how to run a command (`make`, `npm run`, an editor's integrated terminal,
    // `git rebase -i`) would launch that other shell instead, and every cash guarantee
    // would stop at that boundary. `$0` already names cash; `$SHELL` now agrees.
    if let Ok(own) = std::env::current_exe() {
        vars.insert("SHELL".to_string(), cash_win32::path::render(&own));
    }

    // Copy TEMP/TMP to TMPDIR if TMPDIR doesn't already exist.
    if !vars.contains_key("TMPDIR")
        && let Some(tmp) = vars.get("TEMP").or_else(|| vars.get("TMP")).cloned()
    {
        vars.insert("TMPDIR".to_string(), tmp);
    }

    vars.into_iter()
}

/// Normalizes the case of well-known environment variable names to their
/// canonical POSIX forms (`Path` → `PATH`, `home` → `HOME`).
///
/// # Arguments
///
/// * `name` - The environment variable name to normalize.
fn normalize_env_name(name: &str) -> String {
    // D31's one table. This had its own, of 8 names to cash-win32's 24, and it was the
    // one used, so `declare -p` showed `ComSpec=` and `SystemRoot=` (XC-6).
    cash_win32::env::canonical_name(name).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(source: &[(&str, &str)]) -> BTreeMap<String, String> {
        collect_host_env_vars(
            source
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string())),
        )
        .collect()
    }

    #[test]
    fn normalize_env_name_canonicalizes_well_known() {
        assert_eq!(normalize_env_name("Path"), "PATH");
        assert_eq!(normalize_env_name("path"), "PATH");
        assert_eq!(normalize_env_name("PATH"), "PATH");
        assert_eq!(normalize_env_name("Home"), "HOME");
        assert_eq!(normalize_env_name("UserProfile"), "USERPROFILE");
        assert_eq!(normalize_env_name("Temp"), "TEMP");
        assert_eq!(normalize_env_name("Tmp"), "TMP");
        assert_eq!(normalize_env_name("TmpDir"), "TMPDIR");
    }

    /// The mixed-case names Windows gives that Git Bash upper-cases (XC-6).
    #[test]
    fn normalize_env_name_upper_cases_what_git_bash_does() {
        for (windows, bash) in [
            ("ComSpec", "COMSPEC"),
            ("SystemRoot", "SYSTEMROOT"),
            ("SystemDrive", "SYSTEMDRIVE"),
            ("windir", "WINDIR"),
            ("ProgramFiles", "PROGRAMFILES"),
            ("CommonProgramFiles", "COMMONPROGRAMFILES"),
        ] {
            assert_eq!(normalize_env_name(windows), bash);
        }
    }

    #[test]
    fn normalize_env_name_leaves_unknown_alone() {
        assert_eq!(normalize_env_name("FOO"), "FOO");
        assert_eq!(normalize_env_name("myVar"), "myVar");
        // Nor the Windows names Git Bash leaves as they are.
        for name in ["OneDrive", "ProgramData", "PSModulePath", "ProgramW6432"] {
            assert_eq!(normalize_env_name(name), name);
        }
    }

    #[test]
    fn synthesizes_home_from_userprofile() {
        let vars = run(&[("UserProfile", r"C:\Users\reuben")]);

        // cash (D3): HOME is rendered canonically, so it agrees with $PWD and with
        // everything derived from it — ~ expansion, $HISTFILE, the .cashrc path.
        assert_eq!(
            vars.get("HOME").map(String::as_str),
            Some("C:/Users/reuben")
        );

        // USERPROFILE keeps exactly the spelling Windows gave it: D5 makes PATH the
        // only variable cash translates.
        assert_eq!(
            vars.get("USERPROFILE").map(String::as_str),
            Some(r"C:\Users\reuben")
        );
    }

    #[test]
    fn synthesizes_home_from_homedrive_homepath_when_no_userprofile() {
        let vars = run(&[("HomeDrive", "C:"), ("HomePath", r"\Users\reuben")]);
        assert_eq!(
            vars.get("HOME").map(String::as_str),
            Some("C:/Users/reuben")
        );
    }

    #[test]
    fn preserves_existing_home() {
        let vars = run(&[("HOME", "/already/set"), ("UserProfile", r"C:\Users\other")]);
        assert_eq!(vars.get("HOME").map(String::as_str), Some("/already/set"));
    }

    #[test]
    fn copies_temp_to_tmpdir() {
        let vars = run(&[("Temp", r"C:\Windows\Temp")]);
        assert_eq!(
            vars.get("TMPDIR").map(String::as_str),
            Some(r"C:\Windows\Temp")
        );
    }

    #[test]
    fn prefers_temp_over_tmp_for_tmpdir() {
        let vars = run(&[("TEMP", "one"), ("TMP", "two")]);
        assert_eq!(vars.get("TMPDIR").map(String::as_str), Some("one"));
    }

    #[test]
    fn falls_back_to_tmp_when_no_temp() {
        let vars = run(&[("TMP", "two")]);
        assert_eq!(vars.get("TMPDIR").map(String::as_str), Some("two"));
    }

    #[test]
    fn preserves_existing_tmpdir() {
        let vars = run(&[("TMPDIR", "original"), ("TEMP", "other")]);
        assert_eq!(vars.get("TMPDIR").map(String::as_str), Some("original"));
    }

    #[test]
    fn deterministic_iteration_order() {
        // BTreeMap iteration order is determined by the sorted key order,
        // so the outputs must be identical across invocations regardless of
        // input ordering.
        let a = run(&[("Path", "first"), ("ZETA", "zz"), ("Alpha", "aa")]);
        let b = run(&[("ZETA", "zz"), ("Alpha", "aa"), ("Path", "first")]);
        let keys_a: Vec<_> = a.keys().collect();
        let keys_b: Vec<_> = b.keys().collect();
        assert_eq!(keys_a, keys_b);
    }

    #[test]
    fn collision_last_write_wins() {
        // When two source names normalize to the same canonical key,
        // the later one should overwrite the earlier one (and it should not
        // panic).
        let vars = run(&[("Path", "first"), ("PATH", "second")]);
        assert_eq!(vars.get("PATH").map(String::as_str), Some("second"));
    }
}
