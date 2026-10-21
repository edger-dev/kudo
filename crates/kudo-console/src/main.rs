//! The console: `dx serve` for development; the server binary serves the page
//! and its server functions.

fn main() {
    dioxus::launch(kudo_console::app::App);
}
