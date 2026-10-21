//! What is selected: one state value, read by any pane that needs it.
//!
//! implements: ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35@ed06836e3296e0ea5caad3e2ca7a972c911743ad1a4f87f149c069132dadac35

/// The current selection.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Selection {
    /// The selected context's id.
    pub context: Option<String>,
}
