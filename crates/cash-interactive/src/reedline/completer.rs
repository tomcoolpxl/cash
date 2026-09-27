use nu_ansi_term::{Color, Style};
use std::borrow::BorrowMut;

use crate::{carapace, completion, refs};

pub(crate) struct ReedlineCompleter<SE: cash_core::ShellExtensions> {
    pub shell: refs::ShellRef<SE>,
    /// carapace, when it is installed (D63).
    pub carapace: carapace::Cache,
}

impl<SE: cash_core::ShellExtensions> reedline::Completer for ReedlineCompleter<SE> {
    fn complete(&mut self, line: &str, pos: usize) -> reedline::CompletionResult {
        let suggestions = tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.complete_async(line, pos))
        });
        reedline::CompletionResult::fresh(suggestions)
    }
}

/// What to ask carapace, taken from the shell before it is unlocked.
struct CarapaceRequest {
    words: Vec<String>,
    word_start: usize,
    cwd: std::path::PathBuf,
    environment: Vec<(String, String)>,
}

impl<SE: cash_core::ShellExtensions> ReedlineCompleter<SE> {
    async fn complete_async(&mut self, line: &str, pos: usize) -> Vec<reedline::Suggestion> {
        // carapace answers for a command with no completion of its own (D63). It runs with
        // the shell unlocked, and cash's own candidates stand when it has none.
        let request = {
            let shell_ref = self.shell.clone();
            let shell = shell_ref.lock().await;
            self.carapace_request(shell.as_ref(), line, pos)
        };
        if let Some(request) = request
            && let Some(found) = self.carapace.found()
            && let Some(candidates) =
                found.complete(&request.words, &request.cwd, request.environment)
            && !candidates.is_empty()
        {
            return candidates
                .into_iter()
                .map(|candidate| carapace_suggestion(candidate, request.word_start, pos))
                .collect();
        }

        let mut shell_guard = self.shell.lock().await;
        let shell = shell_guard.borrow_mut().as_mut();
        let completions = completion::complete_async(shell, line, pos).await;

        // We're done with the shell, so drop it eagerly.
        drop(shell_guard);

        let insertion_index = completions.insertion_index;
        let delete_count = completions.delete_count;
        let options = completions.options;

        completions
            .candidates
            .into_iter()
            .map(|candidate| {
                Self::to_suggestion(line, candidate, insertion_index, delete_count, &options)
            })
            .collect()
    }

    /// What to ask carapace for the cursor at `pos`, or `None` when carapace has no say:
    /// it is not installed or does not know the command, the command has a completion of
    /// its own (a `complete` spec, or a `complete -D` default for every command), or the
    /// cursor is where cash completes by itself.
    fn carapace_request(
        &mut self,
        shell: &cash_core::Shell<SE>,
        line: &str,
        pos: usize,
    ) -> Option<CarapaceRequest> {
        let (mut words, word_start) = carapace::command_words(line, pos)?;
        let config = shell.completion_config();
        if config.get(&words[0]).is_some() || config.default.is_some() {
            return None;
        }
        words[0] = self.carapace.get(shell)?.completes(&words[0])?;
        Some(CarapaceRequest {
            words,
            word_start,
            cwd: shell.working_dir().to_path_buf(),
            environment: cash_core::commands::exported_environment(shell),
        })
    }

    #[allow(
        clippy::string_slice,
        reason = "all indices + counts are expected to be at char boundaries"
    )]
    fn to_suggestion(
        line: &str,
        mut candidate: String,
        mut insertion_index: usize,
        mut delete_count: usize,
        options: &cash_core::completion::ProcessingOptions,
    ) -> reedline::Suggestion {
        let mut style = Style::new();

        // Special handling for filename completions.
        if options.treat_as_filenames {
            if cash_core::sys::fs::ends_with_path_separator(&candidate) {
                style = style.fg(Color::Green);
            }

            if insertion_index + delete_count <= line.len() {
                let removed = &line[insertion_index..insertion_index + delete_count];
                if let Some(last_sep_index) = cash_core::sys::fs::rfind_path_separator(removed) {
                    if candidate.starts_with(removed) {
                        candidate = candidate.split_off(last_sep_index + 1);
                        insertion_index += last_sep_index + 1;
                        delete_count -= last_sep_index + 1;
                    }
                }
            }
        }

        // See if there's whitespace at the end.
        let append_whitespace = candidate.ends_with(' ');
        if append_whitespace {
            candidate.pop();
        }

        reedline::Suggestion {
            value: candidate,
            description: None,
            style: Some(style),
            extra: None,
            span: reedline::Span {
                start: insertion_index,
                end: insertion_index + delete_count,
            },
            match_indices: None,
            display_override: None,
            append_whitespace,
        }
    }
}

/// A carapace candidate as the menu shows it, replacing the word from `word_start` to the
/// cursor at `pos`. The value is quoted as cash quotes its own candidates (D40).
fn carapace_suggestion(
    candidate: carapace::Candidate,
    word_start: usize,
    pos: usize,
) -> reedline::Suggestion {
    let value = cash_core::escape::quote_if_needed(
        &candidate.value,
        cash_core::escape::QuoteMode::BackslashEscape,
    )
    .into_owned();
    reedline::Suggestion {
        value,
        description: candidate.description,
        style: None,
        extra: None,
        span: reedline::Span {
            start: word_start,
            end: pos,
        },
        match_indices: None,
        display_override: candidate.display,
        append_whitespace: candidate.space_after,
    }
}

#[cfg(test)]
#[cfg(windows)]
mod tests {
    use super::*;

    /// A stand-in for carapace: it completes `fakecmd`, and describes its first candidate
    /// with the argument it was asked about.
    const FAKE_CARAPACE: &str = concat!(
        "@echo off\r\n",
        "if \"%1\"==\"--list\" (\r\n",
        "  echo {\"fakecmd\":[{\"name\":\"fakecmd\"}]}\r\n",
        "  exit /b 0\r\n",
        ")\r\n",
        "echo {\"nospace\":\"\",\"values\":[",
        "{\"value\":\"alpha\",\"display\":\"alpha\",\"description\":\"asked about %~4\"},",
        "{\"value\":\"two words\",\"display\":\"two words\",\"description\":\"\"}]}\r\n",
    );

    async fn completer_with_fake_carapace() -> (
        ReedlineCompleter<cash_core::extensions::DefaultShellExtensions>,
        tempfile::TempDir,
    ) {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("carapace.cmd"), FAKE_CARAPACE).unwrap();
        let mut shell = cash_core::Shell::builder().build().await.unwrap();
        let path = dir.path().to_string_lossy().into_owned();
        shell
            .set_env_global("PATH", cash_core::ShellVariable::new(path))
            .unwrap();
        let completer = ReedlineCompleter {
            shell: std::sync::Arc::new(tokio::sync::Mutex::new(shell)),
            carapace: carapace::Cache::default(),
        };
        (completer, dir)
    }

    /// The suggestions for `line` once the PATH listing has found carapace.
    async fn settled(
        completer: &mut ReedlineCompleter<cash_core::extensions::DefaultShellExtensions>,
        line: &str,
    ) -> Vec<reedline::Suggestion> {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        loop {
            let suggestions = completer.complete_async(line, line.len()).await;
            if suggestions.iter().any(|s| s.value == "alpha") {
                return suggestions;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "carapace was never asked"
            );
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    }

    #[tokio::test]
    async fn a_command_with_no_completion_of_its_own_asks_carapace() {
        let (mut completer, _dir) = completer_with_fake_carapace().await;
        let suggestions = settled(&mut completer, "fakecmd a").await;

        let alpha = &suggestions[0];
        assert_eq!(alpha.description.as_deref(), Some("asked about a"));
        assert_eq!((alpha.span.start, alpha.span.end), (8, 9));
        assert!(alpha.append_whitespace);
        // Quoted as cash quotes its own candidates.
        assert_eq!(suggestions[1].value, "two\\ words");
        assert_eq!(suggestions[1].description, None);
    }

    #[tokio::test]
    async fn an_empty_word_is_asked_about_too() {
        let (mut completer, _dir) = completer_with_fake_carapace().await;
        let suggestions = settled(&mut completer, "fakecmd ").await;
        assert_eq!(suggestions[0].description.as_deref(), Some("asked about "));
        assert_eq!((suggestions[0].span.start, suggestions[0].span.end), (8, 8));
    }

    #[tokio::test]
    async fn a_completion_of_the_commands_own_wins_over_carapace() {
        let (mut completer, _dir) = completer_with_fake_carapace().await;
        let _ = settled(&mut completer, "fakecmd a").await;

        // As `complete -W zulu fakecmd` defines it.
        completer.shell.lock().await.completion_config_mut().set(
            "fakecmd",
            cash_core::completion::Spec {
                word_list: Some("zulu".into()),
                ..Default::default()
            },
        );
        let suggestions = completer.complete_async("fakecmd ", 8).await;
        let values: Vec<&str> = suggestions.iter().map(|s| s.value.as_str()).collect();
        assert_eq!(values, ["zulu"]);
    }
}
