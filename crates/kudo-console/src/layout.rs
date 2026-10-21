//! The arrangement of the console's panes, as a value.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@115b689ab32e5a9a4ace6a21d50e463c4927e1f41c42513e2b0de4a21166cd97

/// What a pane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PaneKind {
    /// Nodes, contexts and the workers under them.
    Where,
    /// The selected context's head, as a worker sees it.
    Head,
    /// The selected context's trace, newest first.
    Trace,
    /// A worker's pane being taken over: its screen, and a way to type.
    Terminal,
}

/// Which way the panes run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Row,
    Column,
}

/// One pane in an arrangement, and its share of the space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Pane {
    pub kind: PaneKind,
    pub grow: u8,
}

/// An arrangement of panes: one value, read from one place.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Layout {
    pub direction: Direction,
    pub panes: &'static [Pane],
}

/// v1: where | head | trace, side by side.
pub const THREE_PANES: Layout = Layout {
    direction: Direction::Row,
    panes: &[
        Pane {
            kind: PaneKind::Where,
            grow: 1,
        },
        Pane {
            kind: PaneKind::Head,
            grow: 2,
        },
        Pane {
            kind: PaneKind::Trace,
            grow: 2,
        },
        Pane {
            kind: PaneKind::Terminal,
            grow: 2,
        },
    ],
};

/// The same panes, stacked, the head first — a phone-shaped arrangement.
pub const STACKED: Layout = Layout {
    direction: Direction::Column,
    panes: &[
        Pane {
            kind: PaneKind::Head,
            grow: 1,
        },
        Pane {
            kind: PaneKind::Where,
            grow: 1,
        },
        Pane {
            kind: PaneKind::Trace,
            grow: 1,
        },
        Pane {
            kind: PaneKind::Terminal,
            grow: 1,
        },
    ],
};
