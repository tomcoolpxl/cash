//! `croot`, the file and folder picker cash opens below the command line on Alt-E, and
//! as the `croot` command (spec D73).
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

pub mod colours;
pub mod context;
pub mod filter;
pub mod pick;
pub mod search;
pub mod tree;
pub mod ui;
