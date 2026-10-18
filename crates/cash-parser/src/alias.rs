//! Aliases, expanded as Bash expands them: on the tokens a command line is read into,
//! before they are parsed.
//!
//! cash expanded an alias after a simple command's words were parsed, so only words could
//! come of it: `alias gs='git status && git diff'` ran `git status && git diff` as one
//! command's words. Bash substitutes the alias's text as the line is read, so its `&&`,
//! `;` and `|` are operators, an alias can open a compound command, and an alias defined
//! on a line applies from the next line on.

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::tokenizer::{Token, TokenizerOptions, tokenize_str_with_options};

/// The aliases a parse expands, with a number that names this set of them.
///
/// Telling two sets apart, or hashing one, as a parse's options are for each lookup in
/// its memo, looks at the number alone. A set changed is a new number.
#[derive(Clone, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Aliases {
    names: Arc<BTreeMap<String, String>>,
    #[cfg_attr(feature = "serde", serde(skip))]
    version: u64,
}

impl Default for Aliases {
    fn default() -> Self {
        Self::new(Arc::default())
    }
}

/// The number the next set of aliases takes.
static NEXT_VERSION: AtomicU64 = AtomicU64::new(1);

impl Aliases {
    /// A set of aliases, named by a number no other set has.
    #[must_use]
    pub fn new(names: Arc<BTreeMap<String, String>>) -> Self {
        Self {
            names,
            version: NEXT_VERSION.fetch_add(1, Ordering::Relaxed),
        }
    }

    /// The text `name` stands for, if it names an alias.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&str> {
        self.names.get(name).map(String::as_str)
    }

    /// Whether there are no aliases.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// The aliases by name.
    #[must_use]
    pub fn names(&self) -> &BTreeMap<String, String> {
        &self.names
    }

    /// The aliases by name, to change: a copy when another holds them too, and a new
    /// number either way, so that a parse made with the old set is told from one made with
    /// the new.
    pub fn names_mut(&mut self) -> &mut BTreeMap<String, String> {
        self.version = NEXT_VERSION.fetch_add(1, Ordering::Relaxed);
        Arc::make_mut(&mut self.names)
    }
}

impl PartialEq for Aliases {
    fn eq(&self, other: &Self) -> bool {
        self.version == other.version
    }
}

impl Eq for Aliases {}

impl std::hash::Hash for Aliases {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.version.hash(state);
    }
}

/// Where the reading of a command line stands, for which word may name an alias.
#[derive(Default)]
struct Reading {
    /// The next word may begin a command: at the start, after a control operator, after a
    /// reserved word that a command follows, after assignments.
    command_position: bool,
    /// The next word is checked whatever its place, as the one after an alias whose text
    /// ends in a blank is.
    check_next: bool,
    /// Words of a redirection still to come (its target, or a here-document's tag, body
    /// and end tag), which name no command.
    redirection_words: usize,
    /// The words before `case`'s `in`, before `for`'s and `select`'s line ends, and inside
    /// `[[ ]]`, which name no command.
    not_commands: bool,
    /// Inside `case … in`, how many are open.
    case_depth: usize,
    /// A case pattern comes next, until its `)`.
    expect_pattern: bool,
    /// The last word was `case`'s subject: the `in` that follows opens the patterns.
    after_case_subject: bool,
    /// After `case`, before its subject.
    after_case: bool,
    /// After `function`, before the name, which a body follows.
    after_function: bool,
}

impl Reading {
    fn start() -> Self {
        Self {
            command_position: true,
            ..Self::default()
        }
    }
}

/// `tokens` with their aliases expanded.
///
/// Every word that names an alias, where Bash looks for one, is replaced by the tokens of
/// the alias's text, those read in their turn; an alias being expanded is not looked for
/// again in its own text.
#[must_use]
pub fn expand(tokens: &[Token], aliases: &Aliases, options: &TokenizerOptions) -> Vec<Token> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut reading = Reading::start();
    let mut expanding = Vec::new();
    expand_into(
        tokens.iter().cloned(),
        aliases,
        options,
        &mut reading,
        &mut expanding,
        &mut out,
    );
    out
}

fn expand_into(
    tokens: impl Iterator<Item = Token>,
    aliases: &Aliases,
    options: &TokenizerOptions,
    reading: &mut Reading,
    expanding: &mut Vec<String>,
    out: &mut Vec<Token>,
) {
    for token in tokens {
        match &token {
            Token::Operator(operator, _) => read_operator(operator, reading),
            Token::Word(word, span) => {
                if let Some(text) = alias_text(word, aliases, reading, expanding) {
                    let ends_in_blank = text.ends_with([' ', '\t']);
                    let alias_tokens: Vec<Token> = tokenize_str_with_options(&text, options)
                        .map(|tokens| tokens.into_iter().map(|token| at(token, span)).collect())
                        .unwrap_or_default();
                    expanding.push(word.clone());
                    // The first word of an alias's text is looked up in its turn, wherever
                    // the alias stood.
                    reading.check_next = true;
                    expand_into(
                        alias_tokens.into_iter(),
                        aliases,
                        options,
                        reading,
                        expanding,
                        out,
                    );
                    expanding.pop();
                    if ends_in_blank {
                        reading.check_next = true;
                    }
                    continue;
                }
                read_word(word, reading);
            }
        }
        out.push(token);
    }
}

/// The text the word stands for, when it is one Bash looks up as an alias here.
fn alias_text(
    word: &str,
    aliases: &Aliases,
    reading: &Reading,
    expanding: &[String],
) -> Option<String> {
    let looked_up = reading.check_next
        || (reading.command_position
            && reading.redirection_words == 0
            && !reading.not_commands
            && !reading.expect_pattern
            && !reading.after_case
            && !reading.after_function);
    if !looked_up || !may_name_an_alias(word) || is_assignment(word) {
        return None;
    }
    if expanding.iter().any(|name| name == word) {
        return None;
    }
    aliases.get(word).map(str::to_owned)
}

/// Whether a word, as written, may name an alias: one with no quoting and nothing to
/// expand in it (`\ls`, `'ls'` and `$cmd` never do).
fn may_name_an_alias(word: &str) -> bool {
    !word.is_empty() && !word.contains(['\\', '\'', '"', '$', '`', '='])
}

/// Whether the word is an assignment, `name=…`, `name+=…` or `name[…]=…`, which may come
/// before a command's name.
fn is_assignment(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let name = name.trim_end_matches('+');
    let name = name.split_once('[').map_or(name, |(name, _)| name);
    !name.is_empty()
        && name
            .chars()
            .next()
            .is_some_and(|first| first == '_' || first.is_ascii_alphabetic())
        && name.chars().all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// A token of an alias's text, placed where the alias's word was, so that what is said
/// about it points there.
fn at(token: Token, span: &crate::SourceSpan) -> Token {
    match token {
        Token::Operator(operator, _) => Token::Operator(operator, span.clone()),
        Token::Word(word, _) => Token::Word(word, span.clone()),
    }
}

fn read_operator(operator: &str, reading: &mut Reading) {
    reading.check_next = false;
    match operator {
        "<<" | "<<-" => reading.redirection_words = 3,
        "<" | ">" | ">>" | ">|" | "<>" | "<&" | ">&" | "&>" | "&>>" | "<<<" => {
            reading.redirection_words = 1;
        }
        ";;" | ";&" | ";;&" => {
            reading.command_position = false;
            reading.expect_pattern = reading.case_depth > 0;
        }
        ")" if reading.expect_pattern => {
            reading.expect_pattern = false;
            reading.command_position = true;
        }
        "(" if reading.expect_pattern => {}
        "|" if reading.expect_pattern => {}
        "\n" | ";" if reading.not_commands && !reading.after_case_subject => {
            // `for`'s and `select`'s words end with their line; `do` follows.
            reading.not_commands = false;
            reading.command_position = true;
        }
        ";" | "&" | "&&" | "||" | "|" | "|&" | "(" | ")" | "\n" => {
            reading.command_position = true;
        }
        _ => {}
    }
}

fn read_word(word: &str, reading: &mut Reading) {
    reading.check_next = false;
    if reading.redirection_words > 0 {
        reading.redirection_words -= 1;
        return;
    }
    if reading.expect_pattern {
        if word == "esac" {
            close_case(reading);
        }
        return;
    }
    if reading.after_case {
        reading.after_case = false;
        reading.after_case_subject = true;
        return;
    }
    if reading.after_function {
        reading.after_function = false;
        reading.command_position = true;
        return;
    }
    if reading.after_case_subject {
        if word == "in" {
            reading.after_case_subject = false;
            reading.not_commands = false;
            reading.expect_pattern = true;
        }
        return;
    }
    if reading.not_commands {
        if word == "]]" {
            reading.not_commands = false;
            reading.command_position = false;
        }
        return;
    }
    if !reading.command_position {
        return;
    }
    match word {
        // A command follows these.
        "if" | "then" | "else" | "elif" | "do" | "while" | "until" | "!" | "{" | "time" => {}
        "case" => {
            reading.case_depth += 1;
            reading.after_case = true;
            reading.not_commands = true;
            reading.command_position = false;
        }
        "esac" => close_case(reading),
        "for" | "select" | "[[" => {
            reading.not_commands = true;
            reading.command_position = false;
        }
        "function" => {
            reading.after_function = true;
            reading.command_position = false;
        }
        _ if is_assignment(word) => {}
        _ => reading.command_position = false,
    }
}

const fn close_case(reading: &mut Reading) {
    reading.case_depth = reading.case_depth.saturating_sub(1);
    reading.expect_pattern = false;
    reading.command_position = false;
}
