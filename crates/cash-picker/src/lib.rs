//! `croot`, the file and folder picker cash opens below the command line on Alt-E, and
//! as the `croot` command (spec D73); and the list drawn the same way for Ctrl-R's
//! history (D79) and `z -i`'s folders (D80).
//!
//! - [`context`]: what the command line says: the command, the word under the cursor,
//!   and so what the picker lists and what a pick does.
//! - [`pick`]: where the picker starts, and how a pick is written on the line.
//! - [`tree`]: a folder's entries, hidden and `.gitignore`d ones left out, and broot's
//!   layout of several levels trimmed to fit.
//! - [`filter`]: the fuzzy filter typing sets.
//! - [`search`]: the background search that finds entries below the shown levels.
//! - [`colours`]: `LS_COLORS` for entries, `CASH_PICKER_COLORS` for the rest.
//! - [`ui`]: the picker's state, its keys, and the lines of each frame.
//! - [`list`]: a list of lines to pick one from, with the same filter and colours.
//! - [`term`]: either on the terminal, below the command line.

pub mod colours;
pub mod context;
pub mod filter;
pub mod list;
pub mod pick;
pub mod search;
pub mod term;
pub mod tree;
pub mod ui;
