//! # reedline `\|/`
//! # A readline replacement written in Rust
//!
//! Reedline is a project to create a line editor (like bash's `readline` or
//! zsh's `zle`) that supports many of the modern conveniences of CLIs,
//! including syntax highlighting, completions, multiline support, Unicode
//! support, and more.  It is currently primarily developed as the interactive
//! editor for [nushell](https://github.com/nushell/nushell) (starting with
//! `v0.60`) striving to provide a pleasant interactive experience.
//!
//! ## Basic example
//!
//! ```rust,no_run
//! // Create a default reedline object to handle user input
//!
//! use reedline::{DefaultPrompt, Reedline, Signal};
//!
//! let mut line_editor = Reedline::create();
//! let prompt = DefaultPrompt::default();
//!
//! loop {
//!     let sig = line_editor.read_line(&prompt);
//!     match sig {
//!         Ok(Signal::Success(buffer)) => {
//!             println!("We processed: {}", buffer);
//!         }
//!         Ok(Signal::CtrlD) | Ok(Signal::CtrlC) => {
//!             println!("\nAborted!");
//!             break;
//!         }
//!         x => {
//!             println!("Event: {:?}", x);
//!         }
//!     }
//! }
//! ```
//! ## Integrate with custom keybindings
//!
//! ```rust
//! // Configure reedline with custom keybindings
//!
//! //Cargo.toml
//! //    [dependencies]
//! //    crossterm = "*"
//!
//! use {
//!   crossterm::event::{KeyCode, KeyModifiers},
//!   reedline::{default_emacs_keybindings, EditCommand, Reedline, Emacs, ReedlineEvent},
//! };
//!
//! let mut keybindings = default_emacs_keybindings();
//! keybindings.add_binding(
//!     KeyModifiers::ALT,
//!     KeyCode::Char('m'),
//!     ReedlineEvent::Edit(vec![EditCommand::BackspaceWord]),
//! );
//! let edit_mode = Box::new(Emacs::new(keybindings));
//!
//! let mut line_editor = Reedline::create().with_edit_mode(edit_mode);
//! ```
//!
//! ## Integrate with [`History`]
//!
//! ```rust,no_run
//! // Create a reedline object with history support, including history size limits
//!
//! use reedline::{FileBackedHistory, Reedline};
//!
//! let history = Box::new(
//!     FileBackedHistory::with_file(5, "history.txt".into())
//!         .expect("Error configuring history with file"),
//! );
//! let mut line_editor = Reedline::create()
//!     .with_history(history);
//! ```
//!
//! ## Integrate with custom syntax [`Highlighter`]
//!
//! ```rust
//! // Create a reedline object with highlighter support
//!
//! use reedline::{ExampleHighlighter, Reedline};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let mut line_editor =
//! Reedline::create().with_highlighter(Box::new(ExampleHighlighter::new(commands)));
//! ```
//!
//! ## Integrate with custom tab completion
//!
//! ```rust
//! // Create a reedline object with tab completions support
//!
//! use reedline::{default_emacs_keybindings, ColumnarMenu, DefaultCompleter, Emacs, KeyCode, KeyModifiers, Reedline, ReedlineEvent, ReedlineMenu, MenuBuilder};
//!
//! let commands = vec![
//!   "test".into(),
//!   "hello world".into(),
//!   "hello world reedline".into(),
//!   "this is the reedline crate".into(),
//! ];
//! let completer = Box::new(DefaultCompleter::new_with_wordlen(commands.clone(), 2));
//! // Use the interactive menu to select options from the completer
//! let completion_menu = Box::new(ColumnarMenu::default().with_name("completion_menu"));
//! // Set up the required keybindings
//! let mut keybindings = default_emacs_keybindings();
//! keybindings.add_binding(
//!     KeyModifiers::NONE,
//!     KeyCode::Tab,
//!     ReedlineEvent::UntilFound(vec![
//!         ReedlineEvent::Menu("completion_menu".to_string()),
//!         ReedlineEvent::MenuNext,
//!     ]),
//! );
//!
//! let edit_mode = Box::new(Emacs::new(keybindings));
//!
//! let mut line_editor = Reedline::create()
//!     .with_completer(completer)
//!     .with_menu(ReedlineMenu::EngineCompleter(completion_menu))
//!     .with_edit_mode(edit_mode);
//! ```
//!
//! ## Integrate with [`Hinter`] for fish-style history autosuggestions
//!
//! ```rust
//! // Create a reedline object with in-line hint support
//!
//! //Cargo.toml
//! //    [dependencies]
//! //    nu-ansi-term = "*"
//!
//! use {
//!   nu_ansi_term::{Color, Style},
//!   reedline::{DefaultHinter, Reedline},
//! };
//!
//!
//! let mut line_editor = Reedline::create().with_hinter(Box::new(
//!   DefaultHinter::default()
//!   .with_style(Style::new().italic().fg(Color::LightGray)),
//! ));
//! ```
//!
//!
//! ## Integrate with custom line completion [`Validator`]
//!
//! ```rust
//! // Create a reedline object with line completion validation support
//!
//! use reedline::{DefaultValidator, Reedline};
//!
//! let validator = Box::new(DefaultValidator);
//!
//! let mut line_editor = Reedline::create().with_validator(validator);
//! ```
//!
//! ## Use custom [`EditMode`]
//!
//! ```rust
//! // Create a reedline object with custom edit mode
//! // This can define a keybinding setting or enable vi-emulation
//! use reedline::{
//!     default_vi_insert_keybindings, default_vi_normal_keybindings, EditMode, Reedline, Vi,
//! };
//!
//! let mut line_editor = Reedline::create().with_edit_mode(Box::new(Vi::new(
//!     default_vi_insert_keybindings(),
//!     default_vi_normal_keybindings(),
//! )));
//! ```
//!
//! ## Enable mouse click-to-cursor
//!
//! ```rust,no_run
//! use reedline::{MouseClickMode, Reedline};
//!
//! let mut line_editor =
//!     Reedline::create().with_mouse_click(MouseClickMode::EnabledWithOsc133);
//! ```
//!
//! ## Use `Helix` edit mode
//!
//! Selection-first editing: motions carry the selection, verbs act on it.
//! Requires the `helix` feature (enabled by default), which also gates the
//! types below.
//!
//! ```rust
//! # #[cfg(feature = "helix")] {
//! use reedline::{default_helix_normal_keybindings, Helix, Reedline};
//!
//! let mut normal_keybindings = default_helix_normal_keybindings();
//! // normal_keybindings.add_binding(..);
//!
//! let line_editor = Reedline::create().with_edit_mode(Box::new(
//!     Helix::default().with_normal_keybindings(normal_keybindings),
//! ));
//! # }
//! ```
//!
//! Run `cargo run --example helix` for the mode on its own, or
//! `cargo run --example demo -- --helix` to exercise it against the demo's
//! history and menus.
//!
//! ## Crate features
//!
//! cash: reedline's other features (the system clipboard, bashisms, SQLite history,
//! the external printer, the idle callback) were never on in cash and are gone.
//! - `helix`: Selection-first `Helix`/Kakoune-style edit mode, where a motion moves the selection and a verb acts on it. On by default; the `Helix` type and its keybinding defaults are gated behind it, so `default-features = false` builds compile without the mode.
//!
//! ## Are we prompt yet? (Development status)
//!
//! Reedline has now all the basic features to become the primary line editor for [nushell](https://github.com/nushell/nushell
//! )
//!
//! - General editing functionality, that should feel familiar coming from other shells (e.g. bash, fish, zsh).
//! - Configurable keybindings (emacs-style bindings and basic vi-style).
//! - Configurable prompt
//! - Content-aware syntax highlighting.
//! - Autocompletion (With graphical selection menu or simple cycling inline).
//! - History with interactive search options (optionally persists to file, can support multiple sessions accessing the same file)
//! - Fish-style history autosuggestion hints
//! - Undo support.
//! - Clipboard integration
//! - Line completeness validation for seamless entry of multiline command sequences.
//!
//! ### Areas for future improvements
//!
//! - [ ] Support for Unicode beyond simple left-to-right scripts
//! - [ ] Easier keybinding configuration
//! - [ ] Support for more advanced vi commands
//! - [ ] Visual selection
//! - [ ] Smooth experience if completion or prompt content takes long to compute
//! - [ ] Support for a concurrent output stream from background tasks to be displayed, while the input prompt is active. ("Full duplex" mode)
//!
//! For more ideas check out the [feature discussion](https://github.com/nushell/reedline/issues/63) or hop on the `#reedline` channel of the [nushell discord](https://discordapp.com/invite/NtAbbGn).
//!
//! ### Alternatives
//!
//! For currently more mature Rust line editing check out:
//!
//! - [rustyline](https://crates.io/crates/rustyline)
//!
#![warn(rustdoc::missing_crate_level_docs)]
#![warn(missing_docs)]
// cash: the workspace lints apply here as everywhere, rustc's warnings included; this code
// came from reedline, written to other rules, so the style lints it was not written to are
// allowed rather than rewritten, as for `cash-sed`. The lints for code that can panic
// apply in full; where the editor slices its text at positions it keeps on character
// boundaries, the module says so.
#![allow(
    elided_lifetimes_in_paths,
    clippy::assert_is_empty,
    clippy::branches_sharing_code,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::doc_link_code,
    clippy::doc_markdown,
    clippy::elidable_lifetime_names,
    clippy::equatable_if_let,
    clippy::explicit_deref_methods,
    clippy::explicit_iter_loop,
    clippy::format_collect,
    clippy::ignored_unit_patterns,
    clippy::implicit_clone,
    clippy::inconsistent_struct_constructor,
    clippy::items_after_statements,
    clippy::manual_assert_eq,
    clippy::manual_let_else,
    clippy::manual_string_new,
    clippy::map_unwrap_or,
    clippy::match_bool,
    clippy::match_wildcard_for_single_variants,
    clippy::missing_const_for_fn,
    clippy::missing_panics_doc,
    clippy::needless_for_each,
    clippy::needless_pass_by_ref_mut,
    clippy::needless_pass_by_value,
    clippy::needless_raw_string_hashes,
    clippy::or_fun_call,
    clippy::redundant_clone,
    clippy::ref_option,
    clippy::return_self_not_must_use,
    clippy::semicolon_if_nothing_returned,
    clippy::significant_drop_tightening,
    clippy::single_match_else,
    clippy::string_lit_as_bytes,
    clippy::too_long_first_doc_paragraph,
    clippy::too_many_lines,
    clippy::uninlined_format_args,
    clippy::unnecessary_lazy_evaluations,
    clippy::unnecessary_semicolon,
    clippy::unnecessary_trailing_comma,
    clippy::unnecessary_wraps,
    clippy::unnested_or_patterns,
    clippy::unused_self,
    clippy::use_self,
    reason = "reedline code, not written to the workspace's style lints"
)]
// Tests that return a `Result` may unwrap and assert in it; clippy.toml lets tests
// panic, and these two lints are the same thing seen from a `Result`.
#![cfg_attr(
    test,
    allow(
        clippy::panic_in_result_fn,
        clippy::unwrap_in_result,
        reason = "tests may unwrap and assert"
    )
)]
// #![deny(warnings)]
mod core_editor;
pub use core_editor::{Editor, LineBuffer};

mod enums;
pub use enums::{
    Direction, EditCommand, EditCommandDiscriminants, FindStop, Granularity, MotionTarget,
    MouseButton, ReedlineEvent, ReedlineEventDiscriminants, ReedlineRawEvent, Signal, TextObject,
    TextObjectScope, TextObjectType, UndoBehavior, WordEdge, WordKind,
};

mod painting;
pub use painting::{Painter, StyledText};

mod engine;
pub use engine::{MouseClickMode, Reedline};

mod result;
pub use result::{ReedlineError, ReedlineErrorVariants, Result};

mod history;
pub use history::{
    CommandLineSearch, FileBackedHistory, History, HistoryItem, HistoryItemExtraInfo,
    HistoryItemId, HistoryNavigationQuery, HistorySessionId, IgnoreAllExtraInfo, JsonFilterValue,
    SearchDirection, SearchFilter, SearchQuery, HISTORY_SIZE,
};

mod prompt;
pub use prompt::PromptHelixMode;
pub use prompt::{
    DefaultPrompt, DefaultPromptSegment, Prompt, PromptEditMode, PromptEditModeDiscriminants,
    PromptHistorySearch, PromptHistorySearchStatus, PromptViMode, DEFAULT_INDICATOR_COLOR,
    DEFAULT_PROMPT_COLOR, DEFAULT_PROMPT_MULTILINE_COLOR, DEFAULT_PROMPT_RIGHT_COLOR,
};

mod edit_mode;
pub use edit_mode::{
    default_emacs_keybindings, default_vi_insert_keybindings, default_vi_normal_keybindings,
    CursorConfig, EditMode, Emacs, Keybindings, Vi,
};
pub use edit_mode::{
    default_helix_insert_keybindings, default_helix_normal_keybindings,
    default_helix_select_keybindings, Helix,
};

mod highlighter;
pub use highlighter::{AbbrExpandContext, ExampleHighlighter, Highlighter, SimpleMatchHighlighter};

mod completion;
pub use completion::{
    Completer, CompletionOrigin, CompletionResult, CompletionStatus, DefaultCompleter, Partial,
    Span, Suggestion, Suggestions,
};

mod hinter;
pub use hinter::CwdAwareHinter;
pub use hinter::{DefaultHinter, Hinter};

mod validator;
pub use validator::{DefaultValidator, ValidationResult, Validator};

mod menu;
pub use menu::{
    menu_functions, ColumnarMenu, DescriptionMenu, DescriptionMode, DescriptionPosition, IdeMenu,
    InputMode, ListMenu, Menu, MenuBuilder, MenuEvent, MenuTextStyle, OutputMode, ReedlineMenu,
    TraversalDirection,
};

mod terminal_extensions;
pub use terminal_extensions::kitty_protocol_available;
pub use terminal_extensions::semantic_prompt::{
    Osc133ClickEventsMarkers, Osc133Markers, Osc633Markers, PromptKind, SemanticPromptMarkers,
};

mod utils;

pub use utils::{
    get_reedline_default_keybindings, get_reedline_keybinding_modifiers, get_reedline_keycodes,
};

// Reexport the key types to be independent from an explicit crossterm dependency.
pub use crossterm::event::{KeyCode, KeyModifiers};
pub use nu_ansi_term::Color;

// cash: types the public items above use, named so a caller can name them too.
pub use edit_mode::KeyCombination;
pub use engine::RepaintSignal;
pub use enums::{EditType, EventStatus};
pub use menu::MenuSettings;
pub use painting::PainterSuspendedState;
