//! `#!/usr/bin/env …` lines: GNU `env`'s options, ahead of the command they start.
//!
//! The kernel hands everything after the interpreter to it as one argument, so
//! `#!/usr/bin/env -S bash -e` gives `env` the string `-S bash -e`, and `-S` splits it
//! into words, with quotes, escapes and `${NAME}`. cash split the line on blanks and took
//! `-S` for the command (W32-08). The words are then `env`'s argument list: assignments,
//! `-i`, `-u NAME` and `-C DIR`, up to the command.
//!
//! A line without `-S` is still split on blanks, as before: `#!/usr/bin/env bash -e`
//! fails on Linux and in Git Bash, which look for a command named `bash -e`, and cash
//! runs it.

use std::collections::VecDeque;
use std::ffi::OsStr;

/// What `env` does to the environment, and the folder, before it starts the command.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EnvChanges {
    /// `-i`: start from an empty environment.
    pub ignore_environment: bool,
    /// `-u NAME`, in order.
    pub unset: Vec<String>,
    /// `NAME=VALUE`, in order; after the unsets, as GNU `env` applies them.
    pub assignments: Vec<(String, String)>,
    /// `-C DIR`.
    pub chdir: Option<String>,
}

/// The command a shebang line starts, its arguments, and what `env` changes first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interpreter {
    /// The interpreter, or the command `env` runs.
    pub command: String,
    /// Its arguments from the shebang line, before the script path.
    pub args: Vec<String>,
    /// What `env` changes; nothing for any other interpreter.
    pub changes: EnvChanges,
}

/// The interpreter a shebang line names: `interpreter` and `args` as they are, or for
/// `env` the command its options lead to.
///
/// # Arguments
///
/// * `interpreter` - The program after `#!`.
/// * `args` - The rest of the line, split on blanks.
/// * `line` - The rest of the line as it stands.
///
/// # Errors
///
/// `env`'s complaint, worded as GNU `env` words it, for an unknown option, a missing
/// option value, a `-S` string that does not split, or a line with no command.
pub fn interpreter(interpreter: &str, args: &[String], line: &str) -> Result<Interpreter, String> {
    if !(interpreter == "/usr/bin/env" || interpreter.ends_with("/env")) {
        return Ok(Interpreter {
            command: interpreter.to_string(),
            args: args.to_vec(),
            changes: EnvChanges::default(),
        });
    }
    env_line(line)
}

fn env_line(line: &str) -> Result<Interpreter, String> {
    let mut words: VecDeque<String> = first_argument(line)?.into();
    let mut changes = EnvChanges::default();
    while let Some(word) = words.pop_front() {
        if word == "--" {
            break;
        }
        if let Some(long) = word.strip_prefix("--") {
            let (name, value) = long.split_once('=').map_or((long, None), |(name, value)| {
                (name, Some(value.to_string()))
            });
            let mut value = || {
                value
                    .clone()
                    .or_else(|| words.pop_front())
                    .ok_or_else(|| format!("option '--{name}' requires an argument"))
            };
            match name {
                "ignore-environment" => changes.ignore_environment = true,
                "debug" | "null" => {}
                "unset" => changes.unset.push(value()?),
                "chdir" => changes.chdir = Some(value()?),
                "split-string" => {
                    let split = split(&value()?)?;
                    for word in split.into_iter().rev() {
                        words.push_front(word);
                    }
                }
                _ => return Err(format!("unrecognized option '--{name}'")),
            }
        } else if word == "-" {
            changes.ignore_environment = true;
        } else if let Some(cluster) = word.strip_prefix('-') {
            short_options(cluster, &mut words, &mut changes)?;
        } else if let Some((name, value)) =
            word.split_once('=').filter(|(name, _)| !name.is_empty())
        {
            changes
                .assignments
                .push((name.to_string(), value.to_string()));
        } else {
            words.push_front(word);
            break;
        }
    }
    let Some(command) = words.pop_front() else {
        return Err("no command on the #!/usr/bin/env line".to_string());
    };
    Ok(Interpreter {
        command,
        args: words.into(),
        changes,
    })
}

/// The words of the one argument the kernel hands `env`. Only an option cluster that
/// reaches `-S` makes more than one word of it; anything else is split on blanks.
fn first_argument(line: &str) -> Result<Vec<String>, String> {
    if let Some(value) = line.strip_prefix("--split-string=") {
        return split(value);
    }
    if let Some((flags, value)) = line
        .strip_prefix('-')
        .and_then(|cluster| cluster.split_once('S'))
        .filter(|(flags, _)| flags.chars().all(|c| matches!(c, 'i' | 'v' | '0')))
    {
        let mut words: Vec<String> = flags
            .chars()
            .filter(|&c| c == 'i')
            .map(|_| "-i".to_string())
            .collect();
        words.extend(split(value)?);
        return Ok(words);
    }
    Ok(line.split_whitespace().map(str::to_string).collect())
}

/// One `-xyz` word: flags, and an option that takes the rest of the word or the next one.
fn short_options(
    cluster: &str,
    words: &mut VecDeque<String>,
    changes: &mut EnvChanges,
) -> Result<(), String> {
    for (at, option) in cluster.char_indices() {
        match option {
            'i' => changes.ignore_environment = true,
            'v' | '0' => {}
            'u' | 'C' | 'S' => {
                let rest = cluster.get(at + option.len_utf8()..).unwrap_or_default();
                let value = if rest.is_empty() {
                    words
                        .pop_front()
                        .ok_or_else(|| format!("option requires an argument -- '{option}'"))?
                } else {
                    rest.to_string()
                };
                match option {
                    'u' => changes.unset.push(value),
                    'C' => changes.chdir = Some(value),
                    _ => {
                        for word in split(&value)?.into_iter().rev() {
                            words.push_front(word);
                        }
                    }
                }
                return Ok(());
            }
            other => return Err(format!("invalid option -- '{other}'")),
        }
    }
    Ok(())
}

/// A `-S` string split into words, by uutils' `env`, which follows GNU's rules.
///
/// Its complaints are worded here, as GNU `env` words them: uutils' own wording comes
/// from a translation bundle that only its `env` builtin loads.
fn split(text: &str) -> Result<Vec<String>, String> {
    use uu_env::EnvError;
    use uu_env::native_int_str::{
        from_native_int_representation_owned, to_native_int_representation,
    };
    let native = to_native_int_representation(OsStr::new(text));
    let words = uu_env::split_iterator::split(&native).map_err(|e| match e {
        EnvError::EnvMissingClosingQuote(..) => "no terminating quote in -S string".to_string(),
        EnvError::EnvInvalidBackslashAtEndOfStringInMinusS(..) => {
            "invalid backslash at end of string in -S".to_string()
        }
        EnvError::EnvBackslashCNotAllowedInDoubleQuotes(_) => {
            r"'\c' must not appear in double-quoted -S string".to_string()
        }
        EnvError::EnvInvalidSequenceBackslashXInMinusS(_, c) => {
            format!(r"invalid sequence '\{c}' in -S")
        }
        EnvError::EnvParsingOfVariableMissingClosingBrace(at)
        | EnvError::EnvParsingOfMissingVariable(at)
        | EnvError::EnvParsingOfVariableOnlyBracedName(at)
        | EnvError::EnvParsingOfVariableUnexpectedNumber(at, _) => {
            // From the `$` that began the reference, as GNU `env` points at it.
            let before = native.get(..at).unwrap_or_default();
            let dollar = before
                .iter()
                .rposition(|&c| c == u16::from(b'$'))
                .unwrap_or(at);
            let rest = String::from_utf16_lossy(native.get(dollar..).unwrap_or_default());
            format!("only ${{VARNAME}} expansion is supported, error at: {rest}")
        }
        other => format!("invalid -S string: {other:?}"),
    })?;
    Ok(words
        .into_iter()
        .map(|word| {
            from_native_int_representation_owned(word)
                .to_string_lossy()
                .into_owned()
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(line: &str) -> Result<Interpreter, String> {
        let args: Vec<String> = line.split_whitespace().map(str::to_string).collect();
        interpreter("/usr/bin/env", &args, line)
    }

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn dash_s_splits_the_line_and_the_command_follows_it() {
        let got = env("-S bash -e").unwrap();
        assert_eq!(got.command, "bash");
        assert_eq!(got.args, words(&["-e"]));
        assert_eq!(got.changes, EnvChanges::default());

        let glued = env("-Sbash -x").unwrap();
        assert_eq!(
            (glued.command.as_str(), glued.args),
            ("bash", words(&["-x"]))
        );
    }

    #[test]
    fn dash_s_keeps_quoted_words_whole() {
        let got = env(r#"-S bash -c 'echo "$0 $1"' zero"#).unwrap();
        assert_eq!(got.command, "bash");
        assert_eq!(got.args, words(&["-c", r#"echo "$0 $1""#, "zero"]));
    }

    #[test]
    fn assignments_unsets_and_flags_come_before_the_command() {
        let got = env("-S -i A=1 -u HOME -C /tmp B=2 python3 -u").unwrap();
        assert_eq!(got.command, "python3");
        assert_eq!(got.args, words(&["-u"]));
        assert_eq!(
            got.changes,
            EnvChanges {
                ignore_environment: true,
                unset: words(&["HOME"]),
                assignments: vec![("A".into(), "1".into()), ("B".into(), "2".into())],
                chdir: Some("/tmp".into()),
            }
        );

        let cluster = env("-iS GREETING=hi bash").unwrap();
        assert!(cluster.changes.ignore_environment);
        assert_eq!(
            cluster.changes.assignments,
            vec![("GREETING".into(), "hi".into())]
        );
    }

    #[test]
    fn a_comment_ends_the_split_string() {
        let got = env("-S bash # trailing comment").unwrap();
        assert_eq!((got.command.as_str(), got.args), ("bash", Vec::new()));
    }

    #[test]
    fn without_dash_s_the_line_is_split_on_blanks() {
        let got = env("bash -e").unwrap();
        assert_eq!((got.command.as_str(), got.args), ("bash", words(&["-e"])));
    }

    #[test]
    fn env_complains_as_gnu_env_does() {
        assert_eq!(env("-S -q bash").unwrap_err(), "invalid option -- 'q'");
        assert_eq!(
            env("-S -u").unwrap_err(),
            "option requires an argument -- 'u'"
        );
        assert!(env("-S A=1").is_err());
        assert!(env(r"-S bash 'unclosed").is_err());
    }

    #[test]
    fn other_interpreters_are_left_alone() {
        let got = interpreter("/bin/bash", &words(&["-e"]), "-e").unwrap();
        assert_eq!(
            (got.command.as_str(), got.args),
            ("/bin/bash", words(&["-e"]))
        );
    }
}
