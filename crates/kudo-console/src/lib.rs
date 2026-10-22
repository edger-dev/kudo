//! kudo's console — a Dioxus fullstack face over the context engine and the hub.
//!
//! implements: d7562b1ac2f15d346f0a98561dc3da3351fabcd807f4ce4bbcd614f2f2a73c1f@9ac5369cd207a2da44b4ac1698c0b6725ef7ab4d94798e3411253a0c60d32c29
//! task: 4aef99814b668f7cbb8900eee43e35b67fdb6dbcd18ba196858f6226874cc0e2@a28de5fa3afe0582a7b097a514a1dadbb50bf6adc99ac51a45aa32722fec0c55

pub mod api;
pub mod app;
pub mod badge;
pub mod confirm;
#[cfg(feature = "server")]
pub mod guard;
pub mod history;
pub mod layout;
pub mod model;
pub mod selection;
#[cfg(feature = "server")]
mod server;
pub mod shell;
pub mod style;
pub mod theme;
#[cfg(feature = "server")]
pub mod views;
