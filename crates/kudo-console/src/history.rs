//! The page's history entries, mirrored: which selection each entry this
//! page wrote holds, and where in them the page is — so the in-app back
//! walks the browser's history only when the entry behind is the level being
//! stepped out to, and the phone's back and this one agree.
//!
//! implements: c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410@c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410

use crate::selection::Selection;

/// How a selection change reaches the browser's history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Write {
    /// A new entry, at this index (deeper than the one left).
    Push(usize),
    /// The current entry, rewritten in place.
    Replace(usize),
}

/// The entries this page wrote, oldest first, and the current one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct History {
    entries: Vec<Selection>,
    current: usize,
}

impl Default for History {
    fn default() -> Self {
        Self::new(Selection::default())
    }
}

impl History {
    /// The page's first entry holds `at`.
    pub fn new(at: Selection) -> Self {
        History {
            entries: vec![at],
            current: 0,
        }
    }

    /// The selection the current entry holds.
    pub fn at(&self) -> Option<&Selection> {
        self.entries.get(self.current)
    }

    /// The selection changed in the page: deeper pushes an entry (dropping
    /// any forward ones), anything else rewrites the current one; nothing to
    /// write when the entry already holds it.
    pub fn record(&mut self, now: &Selection) -> Option<Write> {
        if self.at() == Some(now) {
            return None;
        }
        if now.depth() > self.at().map_or(0, Selection::depth) {
            self.entries.truncate(self.current + 1);
            self.entries.push(now.clone());
            self.current += 1;
            Some(Write::Push(self.current))
        } else {
            if let Some(entry) = self.entries.get_mut(self.current) {
                *entry = now.clone();
            }
            Some(Write::Replace(self.current))
        }
    }

    /// The browser moved to the entry with this index (from its state) holding
    /// `at`. An entry this page does not know — a reload, or no index —
    /// starts the mirror over from it; whether that happened (the page then
    /// re-marks the entry as its first).
    pub fn arrived(&mut self, index: Option<usize>, at: Selection) -> bool {
        match index {
            Some(i) if i < self.entries.len() => {
                self.current = i;
                if let Some(entry) = self.entries.get_mut(i) {
                    *entry = at;
                }
                false
            }
            _ => {
                *self = History::new(at);
                true
            }
        }
    }

    /// Whether stepping out of `now` is the browser's back: the entry behind
    /// holds exactly the level stepped out to.
    pub fn back_is_browser_back(&self, now: &Selection) -> bool {
        let mut out = now.clone();
        out.back();
        self.current > 0 && self.entries.get(self.current - 1) == Some(&out)
    }
}
