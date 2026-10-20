//! How the paths a script sees are spelled: `C:/dir` (D3), or with `shopt -s unixpaths`
//! `/c/dir`, as Git Bash spells them (D82).
//!
//! Every path the shell shows a script (`pwd`, `$PWD`, `$OLDPWD`, `$HOME`, `~`, `dirs`,
//! `which`, `type -P`, `$BASH_SOURCE`) is spelled by [`shown`], so that the option is
//! decided in one place. A path the shell keeps for itself, its working directory or a
//! remembered command, keeps the Windows spelling, which comparisons and Windows need;
//! cash opens either spelling (D3). What a Windows program is handed is turned back by
//! [`cash_win32::path::windows_argument`].

pub use cash_win32::path::{UNIX_PATHS_VARIABLE, shown};

/// Whether the cash that started this one passed `unixpaths` on.
#[must_use]
pub fn inherited() -> bool {
    cash_win32::path::unix_paths_inherited()
}
