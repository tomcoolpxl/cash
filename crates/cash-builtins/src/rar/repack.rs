//! rar's progress through a solid archive it changes. Its files are one stream, so the
//! members kept are packed again, said run by run as "Repacking archived files:", and
//! the old members dropped or replaced before a kept one are decoded to go on reading
//! the stream, said as "Analyzing archived files:". When files are only added, the
//! stream goes on from its end: the old members before the first added are analyzed,
//! not packed again. Each count is written over the one before it with backspaces, as
//! rar writes them; `-idp` shows none.

use std::fmt::Write as _;

use super::Rar;

/// The lines of one change of a solid archive, said as its members go by.
pub(super) struct Repack<'r, 'a, SE: cash_core::ShellExtensions> {
    rar: &'r Rar<'a, SE>,
    /// The bytes the archive's files hold once changed, for the share written after a
    /// kept member with data: `a`, `u`, `f` and `m` show it, `d` does not.
    total: Option<u64>,
    /// The bytes of the members written so far.
    done: u64,
    /// The members of the run of kept ones being said, once its line is begun.
    run: Option<u64>,
    /// The old members dropped or replaced since the last kept one.
    pending: u64,
}

impl<'r, 'a, SE: cash_core::ShellExtensions> Repack<'r, 'a, SE> {
    pub(super) const fn new(rar: &'r Rar<'a, SE>, total: Option<u64>) -> Self {
        Self {
            rar,
            total,
            done: 0,
            run: None,
            pending: 0,
        }
    }

    /// A kept member begun: the old ones dropped before it are analyzed first, and its
    /// run's line begun when it is the first.
    pub(super) fn kept_begin(&mut self) {
        if self.pending > 0 {
            self.analyze(self.pending);
            self.pending = 0;
        }
        if self.run.is_none() {
            self.rar
                .console
                .msg(&format!("\nRepacking archived files: {:12}", ""));
            self.run = Some(0);
        }
    }

    /// A kept member of `size` bytes packed again: its count in the run, and the share
    /// of the bytes written so far when it holds any.
    pub(super) fn kept_end(&mut self, size: u64) {
        let count = self.run.unwrap_or(0) + 1;
        self.run = Some(count);
        self.done = self.done.saturating_add(size);
        let mut text = format!("{}{count:>7}     ", "\u{8}".repeat(12));
        if let Some(total) = self.total.filter(|_| size > 0) {
            let share = u128::from(self.done) * 100 / u128::from(total.max(1));
            let _ = write!(text, "{}{:>3}%", "\u{8}".repeat(4), share.min(100));
        }
        self.rar.console.msg(&text);
    }

    /// A kept member, begun and ended.
    pub(super) fn kept(&mut self, size: u64) {
        self.kept_begin();
        self.kept_end(size);
    }

    /// An old member dropped, or replaced by a file of `size` bytes: its own line ends
    /// the run's, and it is analyzed before the next kept one.
    pub(super) const fn dropped(&mut self, size: u64) {
        self.run = None;
        self.pending += 1;
        self.done = self.done.saturating_add(size);
    }

    /// A file added: its line leaves the run as it is, the next kept member's count
    /// written after it, and the old members dropped still to be analyzed.
    pub(super) const fn added(&mut self, size: u64) {
        self.done = self.done.saturating_add(size);
    }

    /// "Analyzing archived files:" counted to `count`.
    pub(super) fn analyze(&self, count: u64) {
        let mut text = String::from("\nAnalyzing archived files: ");
        for number in 1..=count {
            let _ = write!(text, "{number:>7}{}", "\u{8}".repeat(7));
        }
        self.rar.console.msg(&text);
    }
}
