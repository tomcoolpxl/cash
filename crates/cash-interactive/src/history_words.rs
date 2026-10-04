//! Words of a history entry, for `yank-last-arg` (Alt-.) and `yank-nth-arg`.

/// The word of `line` that `yank-last-arg` inserts: the last one when `nth` is `None`,
/// otherwise word `nth`, counted from 0 (the command) or, when negative, back from the end.
///
/// Words are split as Bash's `!$` splits them: quoting is kept, so `ls "My Documents"`
/// yields `"My Documents"`, and operators count as words, so `make &` yields `&`. A line
/// the tokenizer rejects (an unclosed quote) is split on whitespace instead.
#[must_use]
pub fn pick(line: &str, nth: Option<i64>) -> Option<String> {
    let words = split(line);
    let index = match nth {
        None => words.len().checked_sub(1)?,
        Some(n) if n >= 0 => usize::try_from(n).ok()?,
        Some(n) => words
            .len()
            .checked_sub(usize::try_from(n.unsigned_abs()).ok()?)?,
    };
    words.get(index).map(|word| (*word).to_owned())
}

fn split(line: &str) -> Vec<&str> {
    let Ok(tokens) = cash_parser::tokenize_str(line) else {
        return line.split_whitespace().collect();
    };

    // Token locations count characters; slicing needs bytes.
    let offsets = cash_parser::CharByteOffsets::new(line);
    let byte = |char_index: usize| offsets.byte(char_index);

    tokens
        .iter()
        .filter_map(|token| {
            let location = match token {
                cash_parser::Token::Word(_, location)
                | cash_parser::Token::Operator(_, location) => location,
            };
            let text = line.get(byte(location.start.index)..byte(location.end.index))?;
            // A newline between two commands of one entry is a separator, not a word.
            (!text.trim().is_empty()).then_some(text)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_last_word_by_default() {
        assert_eq!(pick("echo one two", None).as_deref(), Some("two"));
        assert_eq!(pick("ls", None).as_deref(), Some("ls"));
        assert_eq!(pick("", None), None);
    }

    #[test]
    fn quoting_is_kept_and_operators_are_words() {
        assert_eq!(
            pick(r#"ls "My Documents""#, None).as_deref(),
            Some(r#""My Documents""#)
        );
        assert_eq!(pick("cat a | wc -l", None).as_deref(), Some("-l"));
        assert_eq!(pick("make &", None).as_deref(), Some("&"));
    }

    #[test]
    fn nth_counts_from_the_command_or_back_from_the_end() {
        assert_eq!(pick("cp src dest", Some(0)).as_deref(), Some("cp"));
        assert_eq!(pick("cp src dest", Some(1)).as_deref(), Some("src"));
        assert_eq!(pick("cp src dest", Some(-1)).as_deref(), Some("dest"));
        assert_eq!(pick("cp src dest", Some(-3)).as_deref(), Some("cp"));
        assert_eq!(pick("cp src dest", Some(3)), None);
        assert_eq!(pick("cp src dest", Some(-4)), None);
    }

    #[test]
    fn an_unclosed_quote_falls_back_to_whitespace() {
        assert_eq!(pick("echo 'half", None).as_deref(), Some("'half"));
    }

    #[test]
    fn multibyte_words_slice_on_character_boundaries() {
        assert_eq!(pick("echo café naïve", None).as_deref(), Some("naïve"));
    }
}
