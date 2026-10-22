//! A page around the shell for rendering tests: the selection and theme it
//! would hold, provided as the page provides them.

#![allow(dead_code, reason = "each test file uses part of this")]

use dioxus::prelude::*;
use kudo_console::layout::Layout;
use kudo_console::model::ConsoleData;
use kudo_console::selection::Selection;
use kudo_console::shell::Shell;
use kudo_console::theme::Theme;

#[component]
fn Page(layout: Layout, data: ConsoleData, selection: Selection, theme: Theme) -> Element {
    use_context_provider(|| Signal::new(selection.clone()));
    use_context_provider(|| Signal::new(theme));
    rsx! { Shell { layout: layout, data: data } }
}

/// The shell's HTML for `data`, with `selection` selected.
pub fn render_with(layout: Layout, data: ConsoleData, selection: Selection) -> String {
    render_themed(layout, data, selection, Theme::System)
}

pub fn render_themed(
    layout: Layout,
    data: ConsoleData,
    selection: Selection,
    theme: Theme,
) -> String {
    let mut dom = VirtualDom::new_with_props(
        Page,
        PageProps {
            layout,
            data,
            selection,
            theme,
        },
    );
    dom.rebuild_in_place();
    dioxus_ssr::render(&dom)
}

/// The panes the page shows, in order, and the one in focus on a phone.
pub fn panes(html: &str) -> Vec<&'static str> {
    let mut found: Vec<(usize, &'static str)> =
        ["where", "head", "trace", "jobs", "job", "terminal"]
            .into_iter()
            .filter_map(|p| html.find(&format!("data-pane=\"{p}\"")).map(|i| (i, p)))
            .collect();
    found.sort();
    found.into_iter().map(|(_, p)| p).collect()
}

/// The pane marked as the one a phone shows.
pub fn focused(html: &str) -> Option<&'static str> {
    ["where", "head", "trace", "jobs", "job", "terminal"]
        .into_iter()
        .find(|p| {
            html.contains(&format!("data-pane=\"{p}\" data-focus=\"true\""))
                || html.contains(&format!("data-focus=\"true\" data-pane=\"{p}\""))
        })
}
