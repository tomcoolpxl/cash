mod base;
mod default;

pub use base::PromptHelixMode;
pub use base::{
    DEFAULT_INDICATOR_COLOR, DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_MULTILINE_COLOR,
    DEFAULT_PROMPT_RIGHT_COLOR, Prompt, PromptEditMode, PromptEditModeDiscriminants,
    PromptHistorySearch, PromptHistorySearchStatus, PromptViMode,
};

pub use default::{DefaultPrompt, DefaultPromptSegment};
