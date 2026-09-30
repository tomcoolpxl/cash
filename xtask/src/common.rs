//! Common utilities shared across xtask commands.
//!
//! This module provides shared functionality for:
//! - Build profile selection (debug vs release)
//! - Workspace root discovery

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::ValueEnum;

/// Build profile to run the tests under.
#[derive(Clone, Copy, Debug, Default, ValueEnum, PartialEq, Eq)]
pub enum BuildProfile {
    /// Debug build (target/debug/).
    #[default]
    Debug,
    /// Release build (target/release/).
    Release,
}

/// Find the workspace root directory.
///
/// This walks up from the xtask crate directory to find the workspace root
/// (the directory containing the top-level Cargo.toml with `[workspace]`).
pub fn find_workspace_root() -> Result<PathBuf> {
    // Start from the xtask crate directory
    let xtask_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // The workspace root is the parent of xtask/
    let workspace_root = xtask_dir
        .parent()
        .context("Failed to find workspace root (parent of xtask)")?;

    Ok(workspace_root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_workspace_root() {
        let root = find_workspace_root();
        assert!(root.is_ok(), "Should find workspace root");
        let root = root.unwrap();
        // The workspace root should contain Cargo.toml
        assert!(root.join("Cargo.toml").exists());
        // And it should contain the xtask directory
        assert!(root.join("xtask").exists());
    }
}
