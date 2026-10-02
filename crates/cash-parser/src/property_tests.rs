//! Random input for every entry point of the parser: whatever a user types or a script
//! holds, parsing may fail but must not panic. A panic here takes the shell down at the
//! prompt (`echo {1..3..99999999999999999999}` did, through an `unwrap` in a `peg!` rule
//! that clippy cannot see).

use crate::{ParserOptions, arithmetic, prompt, tokenizer, word};
use proptest::prelude::*;

/// Text made mostly of the characters shell syntax turns on, so that random input
/// reaches the parser's corners rather than stopping at the first unknown byte.
fn shellish() -> impl Strategy<Value = String> {
    let alphabet: Vec<char> = "${}()[]<>|&;'\"`\\ \n\t*?!@#%^~=+-,.:/0123456789aAzZ_éEOF"
        .chars()
        .collect();
    prop::collection::vec(
        prop_oneof![
            8 => prop::sample::select(alphabet),
            1 => any::<char>(),
        ],
        0..200,
    )
    .prop_map(|chars| chars.into_iter().collect())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn tokenizing_never_panics(input in shellish()) {
        let _ = tokenizer::tokenize_str(&input);
    }

    #[test]
    fn parsing_a_program_never_panics(input in shellish()) {
        let mut parser = crate::Parser::new(input.as_bytes(), &ParserOptions::default());
        let _ = parser.parse_program();
    }

    #[test]
    fn parsing_a_word_never_panics(input in shellish()) {
        let options = ParserOptions::default();
        let _ = word::parse(&input, &options);
        let _ = word::parse_heredoc(&input, &options);
        let _ = word::parse_prompt_word(&input, &options);
        let _ = word::parse_parameter(&input, &options);
        let _ = word::parse_brace_expansions(&input, &options);
        let _ = word::parse_scalar_assignment(&input, &options);
        let _ = word::parse_compound_assignment_value(&input, &options);
    }

    #[test]
    fn parsing_arithmetic_never_panics(input in shellish()) {
        let _ = arithmetic::parse(&input);
    }

    #[test]
    fn parsing_a_prompt_never_panics(input in shellish()) {
        let _ = prompt::parse(&input);
    }

    #[test]
    fn brace_sequences_with_any_numbers_never_panic(
        start in any::<i128>(),
        end in any::<i128>(),
        step in any::<i128>(),
    ) {
        let options = ParserOptions::default();
        let _ = word::parse_brace_expansions(&format!("{{{start}..{end}..{step}}}"), &options);
        let _ = word::parse_brace_expansions(&format!("{{a..z..{step}}}"), &options);
    }
}
