//! The theme: the system's, unless the viewer picks dark or light.
//!
//! implements: 2dd989d6ce0f4135c0d8d3ab6e99e065ed03fe3dbf1827d5621d289458cd4351@2dd989d6ce0f4135c0d8d3ab6e99e065ed03fe3dbf1827d5621d289458cd4351

/// Which theme the viewer picked.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    /// Follow the system's preference.
    #[default]
    System,
    Dark,
    Light,
}

impl Theme {
    /// The toggle's next state: system → dark → light → system.
    pub fn next(self) -> Theme {
        match self {
            Theme::System => Theme::Dark,
            Theme::Dark => Theme::Light,
            Theme::Light => Theme::System,
        }
    }

    /// The root's `data-theme`, absent for the system's.
    pub fn attr(self) -> Option<&'static str> {
        match self {
            Theme::System => None,
            Theme::Dark => Some("dark"),
            Theme::Light => Some("light"),
        }
    }

    /// What the toggle says.
    pub fn label(self) -> &'static str {
        match self {
            Theme::System => "theme: system",
            Theme::Dark => "theme: dark",
            Theme::Light => "theme: light",
        }
    }

    /// As kept in the browser's storage.
    pub fn stored(self) -> &'static str {
        match self {
            Theme::System => "system",
            Theme::Dark => "dark",
            Theme::Light => "light",
        }
    }

    /// Read back from storage; anything unknown is the system's.
    pub fn from_stored(text: &str) -> Theme {
        match text {
            "dark" => Theme::Dark,
            "light" => Theme::Light,
            _ => Theme::System,
        }
    }
}
