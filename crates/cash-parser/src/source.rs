use std::{fmt::Display, sync::Arc};

/// Represents a position in source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub struct SourcePosition {
    /// The 0-based index of the character in the input stream.
    pub index: usize,
    /// The 1-based line number.
    pub line: usize,
    /// The 1-based column number.
    pub column: usize,
}

impl Display for SourcePosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!("{},{}", self.line, self.column))
    }
}

impl SourcePosition {
    /// Returns a new `SourcePosition` offset by the given `SourcePositionOffset`.
    ///
    /// # Arguments
    ///
    /// * `offset` - The offset to apply.
    #[must_use]
    pub const fn offset(&self, offset: &SourcePositionOffset) -> Self {
        Self {
            index: self.index + offset.index,
            line: self.line + offset.line,
            column: if offset.line == 0 {
                self.column + offset.column
            } else {
                offset.column + 1
            },
        }
    }
}

#[cfg(feature = "diagnostics")]
impl From<&SourcePosition> for miette::SourceOffset {
    #[allow(clippy::cast_sign_loss)]
    fn from(position: &SourcePosition) -> Self {
        position.index.into()
    }
}

/// Represents an offset in source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub struct SourcePositionOffset {
    /// The 0-based character offset.
    pub index: usize,
    /// The 0-based line offset.
    pub line: usize,
    /// The 0-based column offset.
    pub column: usize,
}

/// Represents a span within source text.
#[derive(Clone, Default, Debug)]
#[cfg_attr(
    any(test, feature = "serde"),
    derive(PartialEq, Eq, serde::Serialize, serde::Deserialize)
)]
pub struct SourceSpan {
    /// The start position.
    pub start: Arc<SourcePosition>,
    /// The end position of the span (exclusive).
    pub end: Arc<SourcePosition>,
}

impl SourceSpan {
    /// Returns the length of the token in characters.
    pub fn length(&self) -> usize {
        self.end.index - self.start.index
    }
    pub(crate) fn within(start: &Self, end: &Self) -> Self {
        Self {
            start: start.start.clone(),
            end: end.end.clone(),
        }
    }
}

/// The byte offset of each character of a text: a [`SourcePosition`]'s `index` counts
/// characters, and slicing the text takes bytes.
///
/// The highlighter, carapace's word split and history's word split each built this table
/// themselves (PI-20).
pub struct CharByteOffsets {
    /// The byte offset of each character, then the text's length.
    offsets: Vec<usize>,
}

impl CharByteOffsets {
    /// The table for `text`, made in one pass.
    pub fn new(text: &str) -> Self {
        Self {
            offsets: text
                .char_indices()
                .map(|(byte, _)| byte)
                .chain(std::iter::once(text.len()))
                .collect(),
        }
    }

    /// The byte offset of the character at `char_index`; the text's length at or past
    /// its end.
    pub fn byte(&self, char_index: usize) -> usize {
        self.offsets
            .get(char_index)
            .or_else(|| self.offsets.last())
            .copied()
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::CharByteOffsets;

    #[test]
    fn character_indices_become_byte_offsets() {
        let offsets = CharByteOffsets::new("a爸b");
        assert_eq!(
            (0..5).map(|i| offsets.byte(i)).collect::<Vec<_>>(),
            [0, 1, 4, 5, 5]
        );
        assert_eq!(CharByteOffsets::new("").byte(3), 0);
    }
}
