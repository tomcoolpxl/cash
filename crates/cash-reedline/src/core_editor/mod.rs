mod clip_buffer;
mod cursor;
mod edit_stack;
mod editor;
mod graphemes;
mod line;
mod line_buffer;
mod resolve;
mod rest_policy;
mod word;

pub(crate) use clip_buffer::{Clipboard, get_local_clipboard};
pub(crate) use cursor::{CaretGeometry, Cursor, Movement, SelectionExtent};
pub use editor::Editor;
pub(crate) use graphemes::{ensure_grapheme_boundary_next, ensure_grapheme_boundary_prev};
pub use line_buffer::LineBuffer;
pub(crate) use resolve::{operator_span, resolve_motion};
pub(crate) use rest_policy::{RestPolicy, commit, recohere};
