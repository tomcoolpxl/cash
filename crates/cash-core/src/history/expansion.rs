//! Bash-style history expansion (bang-style history substitution).
//!
//! Provides support for event designators (`!$`, `!!`, `!n`, `!-n`, `!string`, `!?string?`, `!#`),
//! quick substitution (`^old^new^`), word designators (`^`, `$`, `*`, `n`, `x-y`, `x*`, `x-`),
//! and modifiers (`:h`, `:t`, `:r`, `:e`, `:p`, `:q`, `:x`, `:s/old/new/`, `:gs/old/new/`, `:&`).

use crate::history::History;

/// The outcome of expanding a command line using shell history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryExpansionResult {
    /// The expanded command line string.
    pub line: String,
    /// Whether any expansion occurred (line changed or quick-substituted).
    pub changed: bool,
    /// Whether the `:p` modifier was applied (print only, do not execute).
    pub print_only: bool,
}

/// An error that occurred during history expansion.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HistoryExpansionError {
    /// An event designator was not found in history (e.g. `!foobar: event not found`).
    #[error("{0}: event not found")]
    EventNotFound(String),

    /// A word specifier was invalid or out of range (e.g. `:5: bad word specifier`).
    #[error("{0}: bad word specifier")]
    BadWordSpecifier(String),

    /// A substitution failed (e.g. `:s^old^new^: substitution failed`).
    #[error("{0}: substitution failed")]
    SubstitutionFailed(String),

    /// An unrecognized modifier was encountered.
    #[error("{0}: unrecognized history modifier")]
    UnrecognizedModifier(char),
}

/// Representation of an event designator.
#[derive(Debug, Clone, PartialEq, Eq)]
enum EventSpec {
    Previous(Option<char>),
    CurrentLine,
    Offset(usize),
    Absolute(usize),
    Contains(String),
    Prefix(String),
}

/// Splits a historical command line into words, respecting single and double quotes,
/// backslash escapes, and shell metacharacters (`|`, `&`, `;`, `<`, `>`, `(`, `)`).
#[must_use]
pub fn split_history_words(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut cur_word = String::new();
    let mut chars = line.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '\'' => {
                cur_word.push(c);
                for inner in chars.by_ref() {
                    cur_word.push(inner);
                    if inner == '\'' {
                        break;
                    }
                }
            }
            '"' => {
                cur_word.push(c);
                while let Some(inner) = chars.next() {
                    cur_word.push(inner);
                    if inner == '\\' {
                        if let Some(esc) = chars.next() {
                            cur_word.push(esc);
                        }
                    } else if inner == '"' {
                        break;
                    }
                }
            }
            '\\' => {
                cur_word.push(c);
                if let Some(next_c) = chars.next() {
                    cur_word.push(next_c);
                }
            }
            ' ' | '\t' | '\r' | '\n' => {
                if !cur_word.is_empty() {
                    words.push(std::mem::take(&mut cur_word));
                }
            }
            '|' | '&' | ';' | '<' | '>' | '(' | ')' => {
                if !cur_word.is_empty() {
                    words.push(std::mem::take(&mut cur_word));
                }
                let mut meta = String::from(c);
                if (c == '|' && chars.peek() == Some(&'|'))
                    || (c == '&' && chars.peek() == Some(&'&'))
                    || (c == '>' && chars.peek() == Some(&'>'))
                    || (c == '<' && chars.peek() == Some(&'<'))
                    || (c == ';' && chars.peek() == Some(&';'))
                    || (c == '|' && chars.peek() == Some(&'&'))
                    || (c == '>' && chars.peek() == Some(&'&'))
                    || (c == '<' && chars.peek() == Some(&'&'))
                {
                    if let Some(next_m) = chars.next() {
                        meta.push(next_m);
                    }
                    if meta == "<<" && chars.peek() == Some(&'<') {
                        if let Some(third_m) = chars.next() {
                            meta.push(third_m);
                        }
                    } else if meta == ";;" && chars.peek() == Some(&'&') {
                        if let Some(third_m) = chars.next() {
                            meta.push(third_m);
                        }
                    }
                }
                words.push(meta);
            }
            _ => {
                cur_word.push(c);
            }
        }
    }

    if !cur_word.is_empty() {
        words.push(cur_word);
    }

    words
}

/// Applies a single pathname modifier (`h`, `t`, `r`, `e`) or quoting modifier (`q`, `x`) to a string.
fn apply_path_modifier(word: &str, modifier: char) -> String {
    match modifier {
        'h' => {
            // Head: strip last pathname component (dirname)
            if let Some(pos) = word.rfind(['/', '\\']) {
                if pos == 0 {
                    word.chars().take(1).collect()
                } else {
                    word.get(..pos)
                        .map_or_else(|| word.to_owned(), ToOwned::to_owned)
                }
            } else {
                word.to_owned()
            }
        }
        't' => {
            // Tail: strip all leading pathname components (basename)
            if let Some(pos) = word.rfind(['/', '\\']) {
                word.get(pos + 1..)
                    .map_or_else(|| word.to_owned(), ToOwned::to_owned)
            } else {
                word.to_owned()
            }
        }
        'r' => {
            // Root: strip trailing .xxx extension
            if let Some(pos) = word.rfind('.') {
                word.get(..pos)
                    .map_or_else(|| word.to_owned(), ToOwned::to_owned)
            } else {
                word.to_owned()
            }
        }
        'e' => {
            // Extension: keep all but the suffix
            if let Some(pos) = word.rfind('.') {
                word.get(pos..).map_or_else(String::new, ToOwned::to_owned)
            } else {
                String::new()
            }
        }
        'q' => {
            // Quote word
            format!("'{word}'")
        }
        'x' => {
            // Quote word breaking at blanks
            word.split_whitespace()
                .map(|p| format!("'{p}'"))
                .collect::<Vec<_>>()
                .join(" ")
        }
        _ => word.to_owned(),
    }
}

/// Attempts quick substitution (`^old^new^` or `^old^new`) if line starts with `^`.
fn try_quick_substitution(
    line: &str,
    history: Option<&History>,
) -> Result<Option<HistoryExpansionResult>, HistoryExpansionError> {
    if !line.starts_with('^') {
        return Ok(None);
    }

    let mut chars = line.chars().skip(1);
    let mut old_str = String::new();
    let mut new_str = String::new();
    let mut suffix = String::new();
    let mut in_new = false;
    let mut done = false;

    for c in chars.by_ref() {
        if !in_new {
            if c == '^' {
                in_new = true;
            } else {
                old_str.push(c);
            }
        } else if !done {
            if c == '^' {
                done = true;
            } else {
                new_str.push(c);
            }
        } else {
            suffix.push(c);
        }
    }

    let hist = history.ok_or_else(|| HistoryExpansionError::EventNotFound("!".to_owned()))?;
    if hist.is_empty() {
        return Err(HistoryExpansionError::EventNotFound("!".to_owned()));
    }
    let last_item = hist
        .get(hist.count() - 1)
        .ok_or_else(|| HistoryExpansionError::EventNotFound("!".to_owned()))?;
    let last_cmd = &last_item.command_line;

    if !last_cmd.contains(&old_str) {
        return Err(HistoryExpansionError::SubstitutionFailed(format!(
            ":s^{old_str}^{new_str}^"
        )));
    }

    let replaced = last_cmd.replacen(&old_str, &new_str, 1);
    let expanded_line = format!("{replaced}{suffix}");

    Ok(Some(HistoryExpansionResult {
        line: expanded_line,
        changed: true,
        print_only: false,
    }))
}

/// Resolves the raw command line for an event designator.
fn resolve_event_text(
    event: &EventSpec,
    line_so_far: &str,
    history: Option<&History>,
) -> Result<String, HistoryExpansionError> {
    match event {
        EventSpec::CurrentLine => Ok(line_so_far.to_owned()),
        EventSpec::Previous(word_char) => {
            let hist = history.ok_or_else(|| {
                let repr = word_char.map_or_else(|| "!".to_owned(), |w| format!("!{w}"));
                HistoryExpansionError::EventNotFound(repr)
            })?;
            if hist.is_empty() {
                let repr = word_char.map_or_else(|| "!".to_owned(), |w| format!("!{w}"));
                return Err(HistoryExpansionError::EventNotFound(repr));
            }
            let item = hist
                .get(hist.count() - 1)
                .ok_or_else(|| HistoryExpansionError::EventNotFound("!".to_owned()))?;
            Ok(item.command_line.clone())
        }
        EventSpec::Offset(n) => {
            let hist =
                history.ok_or_else(|| HistoryExpansionError::EventNotFound(format!("!-{n}")))?;
            if *n == 0 || *n > hist.count() {
                return Err(HistoryExpansionError::EventNotFound(format!("!-{n}")));
            }
            let item = hist
                .get(hist.count() - *n)
                .ok_or_else(|| HistoryExpansionError::EventNotFound(format!("!-{n}")))?;
            Ok(item.command_line.clone())
        }
        EventSpec::Absolute(n) => {
            let hist =
                history.ok_or_else(|| HistoryExpansionError::EventNotFound(format!("!{n}")))?;
            if *n == 0 || *n > hist.count() {
                return Err(HistoryExpansionError::EventNotFound(format!("!{n}")));
            }
            let item = hist
                .get(*n - 1)
                .ok_or_else(|| HistoryExpansionError::EventNotFound(format!("!{n}")))?;
            Ok(item.command_line.clone())
        }
        EventSpec::Contains(s) => {
            let hist =
                history.ok_or_else(|| HistoryExpansionError::EventNotFound(format!("!?{s}?")))?;
            for item in hist.iter().collect::<Vec<_>>().into_iter().rev() {
                if item.command_line.contains(s) {
                    return Ok(item.command_line.clone());
                }
            }
            Err(HistoryExpansionError::EventNotFound(format!("!?{s}?")))
        }
        EventSpec::Prefix(p) => {
            let hist =
                history.ok_or_else(|| HistoryExpansionError::EventNotFound(format!("!{p}")))?;
            for item in hist.iter().collect::<Vec<_>>().into_iter().rev() {
                if item.command_line.starts_with(p) {
                    return Ok(item.command_line.clone());
                }
            }
            Err(HistoryExpansionError::EventNotFound(format!("!{p}")))
        }
    }
}

/// Performs history expansion on a command line string.
///
/// # Errors
///
/// Returns a [`HistoryExpansionError`] if an event or word designator cannot be found
/// or if substitution fails.
#[expect(clippy::too_many_lines, clippy::cognitive_complexity)]
pub fn expand_history(
    line: &str,
    history: Option<&History>,
) -> Result<HistoryExpansionResult, HistoryExpansionError> {
    if let Some(quick_res) = try_quick_substitution(line, history)? {
        return Ok(quick_res);
    }

    let mut result = String::with_capacity(line.len());
    let mut changed = false;
    let mut print_only = false;
    let mut last_subst: Option<(String, String)> = None;

    let chars: Vec<char> = line.chars().collect();
    let len = chars.len();
    let mut i = 0;
    let mut in_single_quotes = false;
    let mut in_double_quotes = false;

    while i < len {
        let c = chars[i];

        if c == '\'' && !in_double_quotes {
            in_single_quotes = !in_single_quotes;
            result.push(c);
            i += 1;
            continue;
        }

        if in_single_quotes {
            result.push(c);
            i += 1;
            continue;
        }

        if c == '"' {
            in_double_quotes = !in_double_quotes;
            result.push(c);
            i += 1;
            continue;
        }

        if c == '\\' {
            if i + 1 < len && chars[i + 1] == '!' {
                result.push('\\');
                result.push('!');
                i += 2;
                continue;
            }
            result.push(c);
            i += 1;
            continue;
        }

        if c == '#' && !in_double_quotes {
            let at_word_start = i == 0 || chars[i - 1] == ' ' || chars[i - 1] == '\t';
            if at_word_start {
                while i < len {
                    result.push(chars[i]);
                    i += 1;
                }
                break;
            }
        }

        if c == '!' {
            let peek = chars.get(i + 1).copied();
            let is_event = match peek {
                None => false,
                Some(' ' | '\t' | '\r' | '\n' | '=' | '(') => false,
                Some('"') if in_double_quotes => false,
                _ => true,
            };

            if !is_event {
                result.push(c);
                i += 1;
                continue;
            }

            i += 1; // Consume '!'
            let next_ch = chars[i];

            let event_spec;
            let mut word_spec_started_without_colon = None;

            if next_ch == '!' {
                event_spec = EventSpec::Previous(None);
                i += 1;
            } else if next_ch == '#' {
                event_spec = EventSpec::CurrentLine;
                i += 1;
            } else if next_ch == '$' || next_ch == '^' || next_ch == '*' || next_ch == '%' {
                event_spec = EventSpec::Previous(Some(next_ch));
                word_spec_started_without_colon = Some(next_ch);
                i += 1;
            } else if next_ch == '-' {
                if i + 1 < len && chars[i + 1].is_ascii_digit() {
                    i += 1;
                    let mut num = 0usize;
                    while i < len && chars[i].is_ascii_digit() {
                        let digit = chars[i] as usize - '0' as usize;
                        num = num.saturating_mul(10).saturating_add(digit);
                        i += 1;
                    }
                    event_spec = EventSpec::Offset(num);
                } else {
                    return Err(HistoryExpansionError::EventNotFound("!-".to_owned()));
                }
            } else if next_ch.is_ascii_digit() {
                let mut num = 0usize;
                while i < len && chars[i].is_ascii_digit() {
                    let digit = chars[i] as usize - '0' as usize;
                    num = num.saturating_mul(10).saturating_add(digit);
                    i += 1;
                }
                event_spec = EventSpec::Absolute(num);
            } else if next_ch == '?' {
                i += 1;
                let mut search_str = String::new();
                while i < len
                    && chars[i] != '?'
                    && chars[i] != ':'
                    && chars[i] != ' '
                    && chars[i] != '\t'
                    && chars[i] != '\n'
                {
                    search_str.push(chars[i]);
                    i += 1;
                }
                if i < len && chars[i] == '?' {
                    i += 1;
                }
                event_spec = EventSpec::Contains(search_str);
            } else if next_ch == ':' {
                event_spec = EventSpec::Previous(None);
            } else {
                let mut prefix = String::new();
                while i < len
                    && chars[i] != ':'
                    && chars[i] != ' '
                    && chars[i] != '\t'
                    && chars[i] != '\n'
                    && chars[i] != '"'
                {
                    prefix.push(chars[i]);
                    i += 1;
                }
                event_spec = EventSpec::Prefix(prefix);
            }

            let event_text = resolve_event_text(&event_spec, &result, history)?;
            let words = split_history_words(&event_text);
            let num_words = words.len();

            let mut selected_text = None;

            if let Some(w) = word_spec_started_without_colon {
                match w {
                    '$' => {
                        if num_words == 0 {
                            return Err(HistoryExpansionError::BadWordSpecifier("$".to_owned()));
                        }
                        selected_text = Some(words[num_words - 1].clone());
                    }
                    '^' => {
                        if num_words < 2 {
                            return Err(HistoryExpansionError::BadWordSpecifier("^".to_owned()));
                        }
                        selected_text = Some(words[1].clone());
                    }
                    '*' => {
                        if num_words <= 1 {
                            selected_text = Some(String::new());
                        } else {
                            selected_text = Some(words[1..].join(" "));
                        }
                    }
                    '%' => {
                        if num_words == 0 {
                            return Err(HistoryExpansionError::BadWordSpecifier("%".to_owned()));
                        }
                        selected_text = Some(words[num_words - 1].clone());
                    }
                    _ => {}
                }
            } else if i < len && chars[i] == ':' {
                if i + 1 < len {
                    let after_colon = chars[i + 1];
                    let is_word_spec = after_colon.is_ascii_digit()
                        || after_colon == '^'
                        || after_colon == '$'
                        || after_colon == '*'
                        || after_colon == '-'
                        || after_colon == '%';

                    if is_word_spec {
                        i += 1; // Consume ':'
                        let mut spec_str = String::new();

                        if chars[i] == '^' {
                            spec_str.push('^');
                            i += 1;
                            if num_words < 2 {
                                return Err(HistoryExpansionError::BadWordSpecifier(
                                    ":^".to_owned(),
                                ));
                            }
                            selected_text = Some(words[1].clone());
                        } else if chars[i] == '$' {
                            spec_str.push('$');
                            i += 1;
                            if num_words == 0 {
                                return Err(HistoryExpansionError::BadWordSpecifier(
                                    ":$".to_owned(),
                                ));
                            }
                            selected_text = Some(words[num_words - 1].clone());
                        } else if chars[i] == '*' {
                            spec_str.push('*');
                            i += 1;
                            if num_words <= 1 {
                                selected_text = Some(String::new());
                            } else {
                                selected_text = Some(words[1..].join(" "));
                            }
                        } else if chars[i] == '-' {
                            spec_str.push('-');
                            i += 1;
                            if i < len && chars[i].is_ascii_digit() {
                                let mut y = 0usize;
                                while i < len && chars[i].is_ascii_digit() {
                                    spec_str.push(chars[i]);
                                    let digit = chars[i] as usize - '0' as usize;
                                    y = y.saturating_mul(10).saturating_add(digit);
                                    i += 1;
                                }
                                if y >= num_words {
                                    return Err(HistoryExpansionError::BadWordSpecifier(format!(
                                        ":{spec_str}"
                                    )));
                                }
                                selected_text = Some(words[0..=y].join(" "));
                            } else if num_words <= 1 {
                                selected_text = Some(String::new());
                            } else {
                                selected_text = Some(words[0..num_words - 1].join(" "));
                            }
                        } else if chars[i].is_ascii_digit() {
                            let mut start_idx = 0usize;
                            while i < len && chars[i].is_ascii_digit() {
                                spec_str.push(chars[i]);
                                let digit = chars[i] as usize - '0' as usize;
                                start_idx = start_idx.saturating_mul(10).saturating_add(digit);
                                i += 1;
                            }

                            if i < len && chars[i] == '-' {
                                spec_str.push('-');
                                i += 1;
                                if i < len && chars[i].is_ascii_digit() {
                                    let mut end_idx = 0usize;
                                    while i < len && chars[i].is_ascii_digit() {
                                        spec_str.push(chars[i]);
                                        let digit = chars[i] as usize - '0' as usize;
                                        end_idx = end_idx.saturating_mul(10).saturating_add(digit);
                                        i += 1;
                                    }
                                    if start_idx >= num_words
                                        || end_idx >= num_words
                                        || start_idx > end_idx
                                    {
                                        return Err(HistoryExpansionError::BadWordSpecifier(
                                            format!(":{spec_str}"),
                                        ));
                                    }
                                    selected_text = Some(words[start_idx..=end_idx].join(" "));
                                } else if i < len && chars[i] == '$' {
                                    spec_str.push('$');
                                    i += 1;
                                    if start_idx >= num_words {
                                        return Err(HistoryExpansionError::BadWordSpecifier(
                                            format!(":{spec_str}"),
                                        ));
                                    }
                                    selected_text = Some(words[start_idx..].join(" "));
                                } else {
                                    if num_words <= 1 || start_idx >= num_words - 1 {
                                        return Err(HistoryExpansionError::BadWordSpecifier(
                                            format!(":{spec_str}"),
                                        ));
                                    }
                                    selected_text = Some(words[start_idx..num_words - 1].join(" "));
                                }
                            } else if i < len && chars[i] == '*' {
                                spec_str.push('*');
                                i += 1;
                                if start_idx >= num_words {
                                    return Err(HistoryExpansionError::BadWordSpecifier(format!(
                                        ":{spec_str}"
                                    )));
                                }
                                selected_text = Some(words[start_idx..].join(" "));
                            } else {
                                if start_idx >= num_words {
                                    return Err(HistoryExpansionError::BadWordSpecifier(format!(
                                        ":{spec_str}"
                                    )));
                                }
                                selected_text = Some(words[start_idx].clone());
                            }
                        }
                    }
                }
            }

            let mut current_text = selected_text.unwrap_or(event_text);

            while i < len && chars[i] == ':' {
                if i + 1 >= len {
                    break;
                }
                let mod_ch = chars[i + 1];
                let is_modifier = matches!(
                    mod_ch,
                    'h' | 't' | 'r' | 'e' | 'p' | 'q' | 'x' | 's' | 'g' | 'a' | 'G' | '&'
                );
                if !is_modifier {
                    break;
                }

                i += 2; // Consume ':' and mod_ch

                let mut global = false;
                let actual_mod = if mod_ch == 'g' || mod_ch == 'a' {
                    global = true;
                    if i < len && (chars[i] == 's' || chars[i] == '&') {
                        let next = chars[i];
                        i += 1;
                        next
                    } else {
                        return Err(HistoryExpansionError::UnrecognizedModifier(mod_ch));
                    }
                } else if mod_ch == 'G' {
                    global = true;
                    if i < len && chars[i] == 's' {
                        i += 1;
                        's'
                    } else {
                        return Err(HistoryExpansionError::UnrecognizedModifier(mod_ch));
                    }
                } else {
                    mod_ch
                };

                match actual_mod {
                    'h' | 't' | 'r' | 'e' | 'q' | 'x' => {
                        current_text = apply_path_modifier(&current_text, actual_mod);
                    }
                    'p' => {
                        print_only = true;
                    }
                    's' => {
                        if i >= len {
                            return Err(HistoryExpansionError::SubstitutionFailed(":s".to_owned()));
                        }
                        let delim = chars[i];
                        i += 1;
                        let mut old_pat = String::new();
                        let mut new_pat = String::new();
                        let mut in_new_pat = false;

                        while i < len {
                            let sc = chars[i];
                            i += 1;
                            if sc == delim {
                                if !in_new_pat {
                                    in_new_pat = true;
                                } else {
                                    break;
                                }
                            } else if !in_new_pat {
                                old_pat.push(sc);
                            } else {
                                new_pat.push(sc);
                            }
                        }

                        if old_pat.is_empty() {
                            if let Some((prev_old, _)) = &last_subst {
                                old_pat.clone_from(prev_old);
                            } else {
                                return Err(HistoryExpansionError::SubstitutionFailed(format!(
                                    ":s{delim}{old_pat}{delim}{new_pat}{delim}"
                                )));
                            }
                        }

                        let mut resolved_new = String::new();
                        let mut new_chars = new_pat.chars().peekable();
                        while let Some(nc) = new_chars.next() {
                            if nc == '\\' && new_chars.peek() == Some(&'&') {
                                resolved_new.push('&');
                                new_chars.next();
                            } else if nc == '&' {
                                resolved_new.push_str(&old_pat);
                            } else {
                                resolved_new.push(nc);
                            }
                        }

                        if !current_text.contains(&old_pat) {
                            return Err(HistoryExpansionError::SubstitutionFailed(format!(
                                ":s{delim}{old_pat}{delim}{new_pat}{delim}"
                            )));
                        }

                        if global {
                            current_text = current_text.replace(&old_pat, &resolved_new);
                        } else {
                            current_text = current_text.replacen(&old_pat, &resolved_new, 1);
                        }

                        last_subst = Some((old_pat, resolved_new));
                    }
                    '&' => {
                        if let Some((old_pat, resolved_new)) = &last_subst {
                            if !current_text.contains(old_pat) {
                                return Err(HistoryExpansionError::SubstitutionFailed(
                                    ":&".to_owned(),
                                ));
                            }
                            if global {
                                current_text = current_text.replace(old_pat, resolved_new);
                            } else {
                                current_text = current_text.replacen(old_pat, resolved_new, 1);
                            }
                        } else {
                            return Err(HistoryExpansionError::SubstitutionFailed(":&".to_owned()));
                        }
                    }
                    _ => {
                        return Err(HistoryExpansionError::UnrecognizedModifier(actual_mod));
                    }
                }
            }

            result.push_str(&current_text);
            changed = true;
            continue;
        }

        result.push(c);
        i += 1;
    }

    Ok(HistoryExpansionResult {
        line: result,
        changed,
        print_only,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_test_history(commands: &[&str]) -> History {
        let mut hist = History::default();
        for &cmd in commands {
            let _ = hist.add(crate::history::Item::new(cmd));
        }
        hist
    }

    #[test]
    fn test_expand_last_arg_bang_dollar() {
        let hist = make_test_history(&["touch foo.txt", "ls -la /tmp/bar"]);
        let res = expand_history("cat !$", Some(&hist)).unwrap();
        assert_eq!(res.line, "cat /tmp/bar");
        assert!(res.changed);
        assert!(!res.print_only);
    }

    #[test]
    fn test_expand_first_arg_bang_caret() {
        let hist = make_test_history(&["ls -la /tmp/bar"]);
        let res = expand_history("echo !^", Some(&hist)).unwrap();
        assert_eq!(res.line, "echo -la");
        assert!(res.changed);
    }

    #[test]
    fn test_expand_all_args_bang_star() {
        let hist = make_test_history(&["mkdir dir1 dir2 dir3"]);
        let res = expand_history("rmdir !*", Some(&hist)).unwrap();
        assert_eq!(res.line, "rmdir dir1 dir2 dir3");
        assert!(res.changed);
    }

    #[test]
    fn test_expand_previous_command_bang_bang() {
        let hist = make_test_history(&["cargo test"]);
        let res = expand_history("sudo !!", Some(&hist)).unwrap();
        assert_eq!(res.line, "sudo cargo test");
        assert!(res.changed);
    }

    #[test]
    fn test_expand_nth_word() {
        let hist = make_test_history(&["echo a b c d"]);
        let res = expand_history("echo !:2 and !:4", Some(&hist)).unwrap();
        assert_eq!(res.line, "echo b and d");
        assert!(res.changed);
    }

    #[test]
    fn test_expand_word_range() {
        let hist = make_test_history(&["echo a b c d"]);
        let res = expand_history("echo !:1-3", Some(&hist)).unwrap();
        assert_eq!(res.line, "echo a b c");
        assert!(res.changed);
    }

    #[test]
    fn test_expand_prefix_search() {
        let hist = make_test_history(&["cargo check", "git status", "ls /tmp"]);
        let res = expand_history("!git", Some(&hist)).unwrap();
        assert_eq!(res.line, "git status");
        assert!(res.changed);
    }

    #[test]
    fn test_expand_contains_search() {
        let hist = make_test_history(&["cargo test --lib", "ls /tmp", "git checkout master"]);
        let res = expand_history("!?checkout?", Some(&hist)).unwrap();
        assert_eq!(res.line, "git checkout master");
        assert!(res.changed);
    }

    #[test]
    fn test_quick_substitution() {
        let hist = make_test_history(&["echo hello world"]);
        let res = expand_history("^hello^goodbye^", Some(&hist)).unwrap();
        assert_eq!(res.line, "echo goodbye world");
        assert!(res.changed);
    }

    #[test]
    fn test_quick_substitution_with_suffix() {
        let hist = make_test_history(&["echo hello"]);
        let res = expand_history("^hello^goodbye^ world", Some(&hist)).unwrap();
        assert_eq!(res.line, "echo goodbye world");
        assert!(res.changed);
    }

    #[test]
    fn test_modifiers_head_and_tail() {
        let hist = make_test_history(&["cat /path/to/my_file.txt"]);
        let res_h = expand_history("echo !$:h", Some(&hist)).unwrap();
        assert_eq!(res_h.line, "echo /path/to");

        let res_t = expand_history("echo !$:t", Some(&hist)).unwrap();
        assert_eq!(res_t.line, "echo my_file.txt");
    }

    #[test]
    fn test_modifiers_root_and_extension() {
        let hist = make_test_history(&["cat my_file.txt"]);
        let res_r = expand_history("echo !$:r", Some(&hist)).unwrap();
        assert_eq!(res_r.line, "echo my_file");

        let res_e = expand_history("echo !$:e", Some(&hist)).unwrap();
        assert_eq!(res_e.line, "echo .txt");
    }

    #[test]
    fn test_modifier_substitution() {
        let hist = make_test_history(&["echo apple banana apple"]);
        let res = expand_history("!!:s/apple/orange/", Some(&hist)).unwrap();
        assert_eq!(res.line, "echo orange banana apple");

        let res_g = expand_history("!!:gs/apple/orange/", Some(&hist)).unwrap();
        assert_eq!(res_g.line, "echo orange banana orange");
    }

    #[test]
    fn test_current_line_bang_hash() {
        let hist = make_test_history(&["touch file.txt"]);
        let res = expand_history("cp file.txt !#:1.bak", Some(&hist)).unwrap();
        assert_eq!(res.line, "cp file.txt file.txt.bak");
        assert!(res.changed);
    }

    #[test]
    fn test_quoted_and_escaped_bangs() {
        let hist = make_test_history(&["echo hello"]);
        // Single quotes preserve ! verbatim
        let res1 = expand_history("echo '!$'", Some(&hist)).unwrap();
        assert_eq!(res1.line, "echo '!$'");
        assert!(!res1.changed);

        // Escaped \! preserves !
        let res2 = expand_history("echo \\!$", Some(&hist)).unwrap();
        assert_eq!(res2.line, "echo \\!$");
        assert!(!res2.changed);

        // Trailing bang is literal
        let res3 = expand_history("echo hello!", Some(&hist)).unwrap();
        assert_eq!(res3.line, "echo hello!");
        assert!(!res3.changed);

        // Double quotes DO expand
        let res4 = expand_history("echo \"!$\"", Some(&hist)).unwrap();
        assert_eq!(res4.line, "echo \"hello\"");
        assert!(res4.changed);
    }

    #[test]
    fn test_empty_history_error() {
        let hist = History::default();
        let err = expand_history("ls !$", Some(&hist)).unwrap_err();
        assert_eq!(err, HistoryExpansionError::EventNotFound("!$".to_owned()));
    }

    #[test]
    fn test_bad_word_specifier() {
        let hist = make_test_history(&["pwd"]);
        let err = expand_history("echo !^", Some(&hist)).unwrap_err();
        assert_eq!(err, HistoryExpansionError::BadWordSpecifier("^".to_owned()));

        let err2 = expand_history("echo !:5", Some(&hist)).unwrap_err();
        assert_eq!(
            err2,
            HistoryExpansionError::BadWordSpecifier(":5".to_owned())
        );
    }
}
