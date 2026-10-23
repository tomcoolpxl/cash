//! Big letters from small ones: FIGlet's and TOIlet's fonts and rendering, for cash's
//! `figlet` builtin (decisions.md D84, research/figlet-design.md).
//!
//! [`font`] reads FIGfont 2.0 files in all their flavours (figlet's `.flf`, toilet's
//! UTF-8 `.tlf` with colour glyphs, zipped or not), [`control`] reads `.flc` control
//! files, [`render`] lays text out as figlet 2.2.5 does (smushing, kerning, wrapping,
//! justification, paragraphs, direction), into a [`canvas::Figure`] of cells, which
//! [`export`] writes out. [`builtin`] holds the fonts cash carries.
//!
//! The crate knows no shell and prints nothing; the builtin reads the command line,
//! finds the fonts and writes the result.

pub mod builtin;
pub mod canvas;
pub mod control;
pub mod export;
pub mod font;
pub mod render;
pub mod style;

#[cfg(test)]
mod golden;

pub use canvas::{Block, Cell, Figure, Row};
pub use control::Controls;
pub use font::{Font, FontError};
pub use render::{Direction, Justify, Layout, Renderer, Settings, render};
pub use style::{Color, Style};
