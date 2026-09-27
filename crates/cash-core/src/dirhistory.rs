//! fish's folder history: where the shell has been, for `prevd`, `nextd`, `cdh` and
//! Alt-← / Alt-→ at the prompt (spec D62).
//!
//! Every change of working folder (`cd`, `pushd`, `popd`) records the folder it left, and
//! forgets the folders ahead of it, as a browser's history does. `prevd` and `nextd` move
//! through the record without adding to it.

use std::path::{Path, PathBuf};

/// How many folders are kept behind the current one, as fish keeps 25.
pub const LIMIT: usize = 25;

/// The folders behind and ahead of the current one.
#[derive(Clone, Debug, Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirectoryHistory {
    /// Oldest first; the last is the folder just left.
    back: Vec<PathBuf>,
    /// Furthest first; the last is the folder `nextd` goes to.
    forward: Vec<PathBuf>,
}

impl DirectoryHistory {
    /// Records that the shell left `from` for another folder, other than by stepping
    /// through this history.
    pub fn left(&mut self, from: PathBuf) {
        if self.back.last() != Some(&from) {
            self.back.push(from);
        }
        if self.back.len() > LIMIT {
            let excess = self.back.len() - LIMIT;
            self.back.drain(..excess);
        }
        self.forward.clear();
    }

    /// The folder `steps` places away: back through history when negative, forward when
    /// positive; `None` if the history does not reach that far.
    #[must_use]
    pub fn target(&self, steps: isize) -> Option<&Path> {
        let (list, count) = self.side(steps);
        let index = list.len().checked_sub(count)?;
        (count > 0).then(|| list[index].as_path())
    }

    /// Records a move of `steps` places (see [`Self::target`]) away from `current`, after
    /// the shell has moved there.
    pub fn moved(&mut self, current: PathBuf, steps: isize) {
        let mut current = current;
        for _ in 0..steps.unsigned_abs() {
            let (from, to) = if steps < 0 {
                (&mut self.back, &mut self.forward)
            } else {
                (&mut self.forward, &mut self.back)
            };
            let Some(next) = from.pop() else {
                return;
            };
            to.push(std::mem::replace(&mut current, next));
        }
    }

    /// The folders behind the current one, oldest first.
    #[must_use]
    pub fn back(&self) -> &[PathBuf] {
        &self.back
    }

    /// The folders ahead of the current one, the next one first.
    pub fn forward(&self) -> impl Iterator<Item = &PathBuf> {
        self.forward.iter().rev()
    }

    fn side(&self, steps: isize) -> (&[PathBuf], usize) {
        if steps < 0 {
            (&self.back, steps.unsigned_abs())
        } else {
            (&self.forward, steps.unsigned_abs())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(name: &str) -> PathBuf {
        PathBuf::from(name)
    }

    /// History after visiting a, b, c and arriving at d.
    fn visited() -> DirectoryHistory {
        let mut history = DirectoryHistory::default();
        for dir in ["a", "b", "c"] {
            history.left(p(dir));
        }
        history
    }

    #[test]
    fn back_and_forward_retrace_the_route() {
        let mut history = visited();
        assert_eq!(history.target(-1), Some(Path::new("c")));
        assert_eq!(history.target(-3), Some(Path::new("a")));
        assert_eq!(history.target(-4), None);
        assert_eq!(history.target(1), None);

        history.moved(p("d"), -2);
        assert_eq!(history.back(), [p("a")]);
        assert_eq!(history.forward().collect::<Vec<_>>(), [&p("c"), &p("d")]);

        history.moved(p("b"), 1);
        assert_eq!(history.back(), [p("a"), p("b")]);
        assert_eq!(history.target(1), Some(Path::new("d")));
    }

    #[test]
    fn leaving_forgets_the_way_forward() {
        let mut history = visited();
        history.moved(p("d"), -1);
        history.left(p("c"));
        assert_eq!(history.forward().count(), 0);
        assert_eq!(history.back(), [p("a"), p("b"), p("c")]);
    }

    #[test]
    fn it_keeps_the_last_twenty_five() {
        let mut history = DirectoryHistory::default();
        for i in 0..30 {
            history.left(p(&i.to_string()));
        }
        assert_eq!(history.back().len(), LIMIT);
        assert_eq!(history.back()[0], p("5"));
    }

    #[test]
    fn leaving_the_same_folder_twice_records_it_once() {
        let mut history = DirectoryHistory::default();
        history.left(p("a"));
        history.left(p("a"));
        assert_eq!(history.back(), [p("a")]);
    }
}
