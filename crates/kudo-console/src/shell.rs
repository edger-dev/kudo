//! The shell: places panes by the layout value and renders each by its kind.

use dioxus::prelude::*;

use crate::layout::Layout;
use crate::model::ConsoleData;

/// The panes of `layout`, drawn from `data` (stub).
#[component]
pub fn Shell(layout: Layout, data: ConsoleData) -> Element {
    let _ = (layout, data);
    rsx! { div {} }
}
