//! `kudo-shim context new | enqueue | show`: making and driving a context by
//! hand, before the console exists.

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_shim::cli;

const T0: i64 = 1_792_500_000;

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| s.to_string()).collect()
}

#[test]
fn new_creates_the_store_and_prints_the_context_id() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let id = cli::context(&dir, &args(&["new", "dogfood"]), T0).unwrap();
    let id = id.trim();
    assert_eq!(id.len(), 64, "a kino id: {id}");

    cli::context(&dir, &args(&["enqueue", id, "write the first test"]), T0).unwrap();
    cli::context(&dir, &args(&["enqueue", id, "make it pass"]), T0).unwrap();
    let shown = cli::context(&dir, &args(&["show", id]), T0).unwrap();
    assert!(shown.contains("# dogfood"), "{shown}");
    assert!(shown.contains("2 ready"), "{shown}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_command_says_how_to_use_it() {
    let dir = std::env::temp_dir();
    let err = cli::context(&dir, &args(&["frobnicate"]), T0).unwrap_err();
    assert!(
        err.contains("new") && err.contains("enqueue") && err.contains("show"),
        "{err}"
    );
}
