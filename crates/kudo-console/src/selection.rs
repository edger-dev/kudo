//! What is selected: one state value, read by any pane that needs it. It
//! decides which pane a phone shows, what back does, and what the URL says.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@115b689ab32e5a9a4ace6a21d50e463c4927e1f41c42513e2b0de4a21166cd97
//! implements: 77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90@77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90
//! implements: c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410@c528784984b47760ebd631769b982975d30c7c10de5c2d98733b7a85571b1410

use crate::layout::PaneKind;
use crate::model::PaneRef;

/// Which of a context's two panes a phone shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tab {
    #[default]
    Head,
    Trace,
}

/// The current selection. A context and a node are two views: selecting one
/// leaves the other.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// The selected context's id.
    pub context: Option<String>,
    /// The context's pane a phone shows.
    pub tab: Tab,
    /// The node whose jobs are shown.
    pub node: Option<String>,
    /// The job on that node whose output is shown.
    pub job: Option<String>,
    /// The pane being taken over.
    pub pane: Option<PaneRef>,
    /// The approvals tray is open.
    pub tray: bool,
}

impl Selection {
    /// Show a context's head and trace.
    pub fn select_context(&mut self, id: String) {
        self.context = Some(id);
        self.tab = Tab::Head;
        self.node = None;
        self.job = None;
    }

    /// Show a node's jobs.
    pub fn select_node(&mut self, node: String) {
        self.node = Some(node);
        self.job = None;
        self.context = None;
        self.tab = Tab::Head;
    }

    /// Show a job of the selected node.
    pub fn select_job(&mut self, job: String) {
        if self.node.is_some() {
            self.job = Some(job);
        }
    }

    /// The pane a phone shows: the deepest thing selected.
    pub fn focus(&self) -> PaneKind {
        if self.pane.is_some() {
            PaneKind::Terminal
        } else if self.job.is_some() {
            PaneKind::Job
        } else if self.node.is_some() {
            PaneKind::Jobs
        } else if self.context.is_some() {
            match self.tab {
                Tab::Head => PaneKind::Head,
                Tab::Trace => PaneKind::Trace,
            }
        } else {
            PaneKind::Where
        }
    }

    /// Whether a pane has anything to show under this selection.
    pub fn shows(&self, kind: PaneKind) -> bool {
        match kind {
            PaneKind::Where => true,
            PaneKind::Head | PaneKind::Trace => self.context.is_some(),
            PaneKind::Jobs => self.node.is_some(),
            PaneKind::Job => self.job.is_some(),
            PaneKind::Terminal => self.pane.is_some(),
        }
    }

    /// How many levels deep it is: deeper is a history push, the same depth
    /// a replace.
    pub fn depth(&self) -> usize {
        [
            self.context.is_some() || self.node.is_some(),
            self.job.is_some(),
            self.pane.is_some(),
            self.tray,
        ]
        .into_iter()
        .filter(|b| *b)
        .count()
    }

    /// Step out one level: the tray, then the pane, the job, the view.
    pub fn back(&mut self) {
        if self.tray {
            self.tray = false;
        } else if self.pane.is_some() {
            self.pane = None;
        } else if self.job.is_some() {
            self.job = None;
        } else {
            *self = Selection::default();
        }
    }

    /// The URL fragment for this selection (`""` for nothing selected).
    pub fn fragment(&self) -> String {
        let mut parts = Vec::new();
        if let Some(c) = &self.context {
            parts.push(format!("c={}", escape(c)));
            if self.tab == Tab::Trace {
                parts.push("t=trace".to_owned());
            }
        }
        if let Some(n) = &self.node {
            parts.push(format!("n={}", escape(n)));
        }
        if let Some(j) = &self.job {
            parts.push(format!("j={}", escape(j)));
        }
        if let Some(p) = &self.pane {
            parts.push(format!("p={}/{}", escape(&p.node), escape(&p.job)));
        }
        if self.tray {
            parts.push("tray=1".to_owned());
        }
        if parts.is_empty() {
            String::new()
        } else {
            format!("#{}", parts.join("&"))
        }
    }

    /// The selection a fragment names; nothing for a fragment that does not
    /// parse whole — a stale or broken link lands on the list, never half a
    /// place.
    pub fn from_fragment(text: &str) -> Selection {
        Self::parse(text).unwrap_or_default()
    }

    fn parse(text: &str) -> Option<Selection> {
        let text = text.strip_prefix('#').unwrap_or(text);
        let mut s = Selection::default();
        if text.is_empty() {
            return Some(s);
        }
        for part in text.split('&') {
            let (key, value) = part.split_once('=')?;
            match key {
                "c" => s.context = Some(unescape(value)?),
                "t" if value == "trace" => s.tab = Tab::Trace,
                "n" => s.node = Some(unescape(value)?),
                "j" => s.job = Some(unescape(value)?),
                "p" => {
                    let (node, job) = value.split_once('/')?;
                    s.pane = Some(PaneRef {
                        node: unescape(node)?,
                        job: unescape(job)?,
                    });
                }
                "tray" if value == "1" => s.tray = true,
                _ => return None,
            }
        }
        let empty = |v: &Option<String>| v.as_ref().is_some_and(String::is_empty);
        let whole = !(empty(&s.context) || empty(&s.node) || empty(&s.job))
            && (s.job.is_none() || s.node.is_some())
            && !(s.context.is_some() && s.node.is_some())
            && s.pane
                .as_ref()
                .is_none_or(|p| !p.node.is_empty() && !p.job.is_empty());
        whole.then_some(s)
    }
}

/// A value as it sits in the fragment: everything but `[A-Za-z0-9-_.:~]`
/// percent-escaped.
fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.:~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// A fragment value back; `None` for a broken escape or bytes that are not
/// UTF-8.
fn unescape(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = std::str::from_utf8(bytes.get(i + 1..i + 3)?).ok()?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// What the last action said: shown above the panes and in the tray.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notice(pub Option<String>);

/// What the last action led to, when it went: shown in the tray, so a
/// verdict's result is read where it was given.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome(pub Option<String>);
