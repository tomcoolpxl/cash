use crate::{ReadResult, ShellError};

pub(crate) struct NonTermLineReader;

impl super::LineReader for NonTermLineReader {
    fn read_line(
        &self,
        _prompt: Option<&str>,
        _completion_handler: impl FnMut(
            &str,
            usize,
        )
            -> Result<cash_core::completion::Completions, crate::ShellError>,
    ) -> Result<crate::ReadResult, crate::ShellError> {
        use std::io::BufRead as _;

        // A byte at a time, and never through `std::io::stdin()`: the lines after this
        // one are the input of the commands it runs, and what a buffer took of them here
        // they would not see.
        let mut input = String::new();
        let bytes_read = std::io::BufReader::with_capacity(1, cash_win32::stdio::RawStdin)
            .read_line(&mut input)
            .map_err(ShellError::InputError)?;

        if bytes_read == 0 {
            Ok(ReadResult::Eof)
        } else {
            Ok(ReadResult::Input(input))
        }
    }
}
