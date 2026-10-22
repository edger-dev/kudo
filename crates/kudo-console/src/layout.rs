//! The arrangement of the console's panes, as a value.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@115b689ab32e5a9a4ace6a21d50e463c4927e1f41c42513e2b0de4a21166cd97
//! implements: 77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90@77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90

/// What a pane shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PaneKind {
    /// Nodes, contexts and the workers under them.
    Where,
    /// The selected context's head, as a worker sees it.
    Head,
    /// The selected context's trace, newest first.
    Trace,
    /// A node's governed jobs.
    Jobs,
    /// One job's output, followed.
    Job,
    /// A worker's pane being taken over: its screen, and a way to type.
    Terminal,
}

/// What a narrow screen (a phone held upright) does with the panes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Narrow {
    /// One pane at a time — the one the selection points at.
    Drill,
    /// The same arrangement as on a wide screen.
    Same,
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
    pub narrow: Narrow,
}

/// v1: where | head | trace, side by side (a node's jobs in place of the
/// head and trace); on a phone, one pane at a time.
pub const THREE_PANES: Layout = Layout {
    direction: Direction::Row,
    narrow: Narrow::Drill,
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
            kind: PaneKind::Jobs,
            grow: 2,
        },
        Pane {
            kind: PaneKind::Job,
            grow: 3,
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
    narrow: Narrow::Same,
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
            kind: PaneKind::Jobs,
            grow: 1,
        },
        Pane {
            kind: PaneKind::Job,
            grow: 1,
        },
        Pane {
            kind: PaneKind::Terminal,
            grow: 1,
        },
    ],
};
