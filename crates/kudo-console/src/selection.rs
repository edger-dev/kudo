//! What is selected: one state value, read by any pane that needs it.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@115b689ab32e5a9a4ace6a21d50e463c4927e1f41c42513e2b0de4a21166cd97

/// The current selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// The selected context's id.
    pub context: Option<String>,
    /// The pane being taken over.
    pub pane: Option<crate::model::PaneRef>,
}

/// What the last action said, when it failed: shown above the panes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Notice(pub Option<String>);
