//! Readline's init file, `~/.inputrc`, as far as cash's line editor can follow it.
//!
//! Git Bash's readline reads `$INPUTRC`, or else `~/.inputrc`, before the first prompt;
//! cash reads the same file, before `~/.bashrc` so that a `bind` there wins as in Bash,
//! and `bind -f FILE` reads one on request. Each line is one readline would read:
//!
//! - `"KEYSEQ": function`, `"KEYSEQ": "macro"`, and the key-name form, `Control-u:
//!   kill-line` or `Meta-Rubout: backward-kill-word`, bind keys in the emacs keymap;
//! - `set editing-mode`, `set keymap`, `set mark-directories` and `set
//!   mark-symlinked-directories` change what cash has of them;
//! - `$if`, `$else`, `$endif` and `$include` work as in readline: `$if` tests `mode=`,
//!   `term=`, `version` (Bash 5.3's readline, 8.3) and the application name, `Bash`.
//!
//! What cash's editor does not have is passed over without a word: a function `bind -l`
//! does not list, a variable it has no use for, a key it cannot name, and every binding
//! in a vi keymap, as cash's vi mode keeps its own keys. A file written for readline
//! then leaves cash's start silent, and binds what it can.

use std::path::PathBuf;

use cash_core::interfaces;

use crate::bind;

/// How deep `$include` may nest, against a file that includes itself.
const MAX_DEPTH: usize = 10;

/// The readline version `$if version` compares with: the one Bash 5.3 ships.
const READLINE_VERSION: (u32, u32) = (8, 3);

/// Reads `$INPUTRC`, or else `~/.inputrc`, into the line editor's bindings, if the shell
/// has a line editor and the file is there; anything wrong with it is passed over.
pub async fn read_startup_file<SE: cash_core::ShellExtensions>(shell: &mut cash_core::Shell<SE>) {
    let Some(helper) = shell.key_bindings().cloned() else {
        return;
    };
    let named = shell
        .env_str("INPUTRC")
        .filter(|value| !value.is_empty())
        .map(|value| value.into_owned());
    let path = match named {
        Some(path) => expand_tilde(shell, &path),
        None => match shell.env_str("HOME") {
            Some(home) => PathBuf::from(home.as_ref()).join(".inputrc"),
            None => return,
        },
    };
    let read = Reader::new(shell, &mut *helper.lock().await).read_file(&path);
    if let Err(error) = read {
        tracing::debug!("couldn't read {}: {error}", path.display());
    }
}

/// Reads `file` into `bindings`, for `bind -f`. Only a file that cannot be read is an
/// error; its lines are taken as at startup.
pub(crate) fn read_file<SE: cash_core::ShellExtensions>(
    shell: &mut cash_core::Shell<SE>,
    bindings: &mut dyn interfaces::KeyBindings,
    file: &str,
) -> std::io::Result<()> {
    let path = expand_tilde(shell, file);
    Reader::new(shell, bindings).read_file(&path)
}

/// `~/` at the start of `path` as `$HOME`, the rest as the shell resolves paths.
fn expand_tilde<SE: cash_core::ShellExtensions>(
    shell: &cash_core::Shell<SE>,
    path: &str,
) -> PathBuf {
    match (path.strip_prefix("~/"), shell.env_str("HOME")) {
        (Some(rest), Some(home)) => PathBuf::from(home.as_ref()).join(rest),
        _ => shell.absolute_path(path),
    }
}

struct Reader<'a, SE: cash_core::ShellExtensions> {
    shell: &'a mut cash_core::Shell<SE>,
    bindings: &'a mut dyn interfaces::KeyBindings,
    /// Whether bindings go to the emacs keymap, the only one cash's editor binds.
    emacs_keymap: bool,
    /// The open `$if`s, innermost last.
    conditions: Vec<Condition>,
    depth: usize,
}

/// One `$if`: whether the lines around it are taken, and what it tested.
struct Condition {
    outer: bool,
    test: bool,
    in_else: bool,
}

impl<'a, SE: cash_core::ShellExtensions> Reader<'a, SE> {
    fn new(
        shell: &'a mut cash_core::Shell<SE>,
        bindings: &'a mut dyn interfaces::KeyBindings,
    ) -> Self {
        let emacs_keymap = !shell.options().vi_mode;
        Self {
            shell,
            bindings,
            emacs_keymap,
            conditions: Vec::new(),
            depth: 0,
        }
    }

    fn read_file(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        let text = std::fs::read_to_string(path)?;
        // An `$if` left open in a file ends with it, as readline has it.
        let open = self.conditions.len();
        for line in text.lines() {
            self.line(line);
        }
        self.conditions.truncate(open);
        Ok(())
    }

    /// Whether lines are taken here, outside every `$if` that tested false.
    fn taking(&self) -> bool {
        self.conditions
            .last()
            .is_none_or(|condition| condition.outer && condition.test != condition.in_else)
    }

    fn line(&mut self, line: &str) {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return;
        }
        if let Some(directive) = line.strip_prefix('$') {
            self.directive(directive);
            return;
        }
        if !self.taking() {
            return;
        }
        if let Some(rest) = strip_word(line, "set") {
            let (name, value) = split_word(rest);
            self.set(name, value);
        } else if self.emacs_keymap
            && let Some(binding) = quoted_binding(line)
            && let Ok((sequence, target)) = bind::parse_key_sequence_and_readline_target(&binding)
        {
            let _ = bind::bind_key_sequence_to_readline_target(self.bindings, sequence, target);
        }
    }

    fn directive(&mut self, directive: &str) {
        let (word, rest) = split_word(directive);
        match word.to_ascii_lowercase().as_str() {
            "if" => {
                let outer = self.taking();
                let test = outer && self.test(rest);
                self.conditions.push(Condition {
                    outer,
                    test,
                    in_else: false,
                });
            }
            "else" => {
                if let Some(condition) = self.conditions.last_mut() {
                    condition.in_else = true;
                }
            }
            "endif" => {
                self.conditions.pop();
            }
            "include" if self.taking() && self.depth < MAX_DEPTH => {
                let path = expand_tilde(self.shell, rest);
                self.depth += 1;
                if let Err(error) = self.read_file(&path) {
                    tracing::debug!("couldn't include {}: {error}", path.display());
                }
                self.depth -= 1;
            }
            _ => {}
        }
    }

    /// What `$if TEST` tests: `mode=`, `term=`, `version OP N` or the application name.
    fn test(&self, test: &str) -> bool {
        let test = test.trim();
        if let Some(mode) = test.strip_prefix("mode=") {
            let current = if self.shell.options().vi_mode {
                "vi"
            } else {
                "emacs"
            };
            mode.trim() == current
        } else if let Some(term) = test.strip_prefix("term=") {
            let term = term.trim();
            self.shell
                .env_str("TERM")
                .is_some_and(|value| value == term || value.split('-').next() == Some(term))
        } else if let Some(rest) = test.strip_prefix("version") {
            version_test(rest)
        } else {
            test.eq_ignore_ascii_case("bash")
        }
    }

    fn set(&mut self, name: &str, value: &str) {
        let value = value.split_whitespace().next().unwrap_or("");
        match name.to_ascii_lowercase().as_str() {
            "editing-mode" if value == "vi" || value == "emacs" => {
                let vi = value == "vi";
                let options = self.shell.options_mut();
                options.vi_mode = vi;
                options.emacs_mode = !vi;
                self.emacs_keymap = !vi;
            }
            "keymap" => self.emacs_keymap = value.starts_with("emacs"),
            "mark-directories" => {
                self.shell
                    .completion_config_mut()
                    .fallback_options
                    .mark_directories = is_on(value);
            }
            "mark-symlinked-directories" => {
                self.shell
                    .completion_config_mut()
                    .fallback_options
                    .mark_symlinked_directories = is_on(value);
            }
            _ => {}
        }
    }
}

/// A readline boolean: `on`, `1` or nothing is on, anything else off.
fn is_on(value: &str) -> bool {
    value.is_empty() || value.eq_ignore_ascii_case("on") || value == "1"
}

/// `rest` after `word` and whitespace, when `line` starts with that word.
fn strip_word<'l>(line: &'l str, word: &str) -> Option<&'l str> {
    let rest = line.strip_prefix(word)?;
    rest.starts_with([' ', '\t']).then(|| rest.trim_start())
}

/// The first word of `text`, and what follows it, trimmed.
fn split_word(text: &str) -> (&str, &str) {
    let text = text.trim();
    text.split_once([' ', '\t'])
        .map_or((text, ""), |(word, rest)| (word, rest.trim()))
}

/// `version OP N` after `version`: `OP` one of `=`, `==`, `!=`, `<`, `<=`, `>`, `>=`, and
/// `N` a version, `8` or `8.1`, compared with readline 8.3.
fn version_test(rest: &str) -> bool {
    let rest = rest.trim();
    let Some(end) = rest.find(|c: char| c.is_ascii_digit()) else {
        return false;
    };
    let (op, number) = rest.split_at(end);
    let mut parts = number.trim().split('.');
    let major = parts.next().and_then(|part| part.parse().ok());
    let minor = parts.next().map_or(Some(0), |part| part.parse().ok());
    let (Some(major), Some(minor)) = (major, minor) else {
        return false;
    };
    let ordering = READLINE_VERSION.cmp(&(major, minor));
    match op.trim() {
        "=" | "==" => ordering.is_eq(),
        "!=" => ordering.is_ne(),
        "<" => ordering.is_lt(),
        "<=" => ordering.is_le(),
        ">" => ordering.is_gt(),
        ">=" => ordering.is_ge(),
        _ => false,
    }
}

/// A binding line as `bind` reads one, `"KEYSEQ": TARGET`: the key-name form written as
/// a key sequence, and anything after a function name, a comment, left out.
fn quoted_binding(line: &str) -> Option<String> {
    let (sequence, rest) = if line.starts_with('"') {
        let end = closing_quote(line)?;
        (line.get(..=end)?.to_owned(), line.get(end + 1..)?)
    } else {
        let (name, rest) = line.split_at(line.find(':')?);
        (format!("\"{}\"", key_name(name.trim())?), rest)
    };
    let target = rest.trim_start().strip_prefix(':')?.trim();
    let target = if target.starts_with('"') {
        target.get(..=closing_quote(target)?)?
    } else {
        target.split_whitespace().next()?
    };
    Some(format!("{sequence}: {target}"))
}

/// The byte index of the quote closing the string that `text` opens, past `\` escapes.
fn closing_quote(text: &str) -> Option<usize> {
    let mut escaped = false;
    for (index, c) in text.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(index),
            _ => {}
        }
    }
    None
}

/// A readline key name, `Control-u`, `C-M-x` or `Meta-Rubout`, as a key sequence:
/// `\C-u`, `\C-\M-x`, `\M-\d`.
fn key_name(name: &str) -> Option<String> {
    let mut sequence = String::new();
    let mut rest = name;
    loop {
        let lower = rest.to_ascii_lowercase();
        if let Some(prefix) = ["control-", "c-"].iter().find(|p| lower.starts_with(**p)) {
            sequence.push_str("\\C-");
            rest = rest.get(prefix.len()..)?;
        } else if let Some(prefix) = ["meta-", "m-"].iter().find(|p| lower.starts_with(**p)) {
            sequence.push_str("\\M-");
            rest = rest.get(prefix.len()..)?;
        } else {
            break;
        }
    }
    let key = match rest.to_ascii_lowercase().as_str() {
        "del" | "rubout" => "\\d",
        "esc" | "escape" => "\\e",
        "lfd" | "newline" => "\\n",
        "ret" | "return" => "\\r",
        "spc" | "space" => " ",
        "tab" => "\\t",
        "\\" => "\\\\",
        "\"" => "\\\"",
        _ if rest.chars().count() == 1 => rest,
        _ => return None,
    };
    sequence.push_str(key);
    Some(sequence)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_names_become_key_sequences() {
        assert_eq!(key_name("Control-u").as_deref(), Some("\\C-u"));
        assert_eq!(key_name("C-M-x").as_deref(), Some("\\C-\\M-x"));
        assert_eq!(key_name("Meta-Rubout").as_deref(), Some("\\M-\\d"));
        assert_eq!(key_name("TAB").as_deref(), Some("\\t"));
        assert_eq!(key_name("Hyper-x"), None);
    }

    #[test]
    fn binding_lines_are_written_as_bind_reads_them() {
        assert_eq!(
            quoted_binding(r#""\C-t": kill-line   # mine"#).as_deref(),
            Some(r#""\C-t": kill-line"#)
        );
        assert_eq!(
            quoted_binding(r#""\e[5~":history-search-backward"#).as_deref(),
            Some(r#""\e[5~": history-search-backward"#)
        );
        assert_eq!(
            quoted_binding(r#"Control-x: "ls\n" trailing"#).as_deref(),
            Some(r#""\C-x": "ls\n""#)
        );
        assert_eq!(
            quoted_binding(r#""\":": self-insert"#).as_deref(),
            Some(r#""\":": self-insert"#)
        );
        assert_eq!(quoted_binding("no colon here"), None);
    }

    #[test]
    fn version_tests_compare_with_readline_8_3() {
        assert!(version_test(">= 8.0"));
        assert!(version_test("==8.3"));
        assert!(version_test("< 9"));
        assert!(!version_test("> 8.3"));
        assert!(!version_test("= 7"));
        assert!(!version_test("about 8"));
    }

    #[test]
    fn readline_booleans() {
        assert!(is_on("on") && is_on("On") && is_on("1") && is_on(""));
        assert!(!is_on("off") && !is_on("yes"));
    }
}
