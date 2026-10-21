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
        err.contains("new")
            && err.contains("enqueue")
            && err.contains("show")
            && err.contains("list"),
        "{err}"
    );
}

#[test]
fn list_shows_each_context_with_its_counts_and_a_prefix_names_one() {
    let dir = std::env::temp_dir().join(format!("kudo-shim-cli-list-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);

    let listed = cli::context(&dir, &args(&["list"]), T0).unwrap();
    assert!(listed.contains("no contexts"), "{listed}");

    let a = cli::context(&dir, &args(&["new", "first", "one"]), T0).unwrap();
    let b = cli::context(&dir, &args(&["new", "second"]), T0 + 60).unwrap();
    let (a, b) = (a.trim(), b.trim());

    // A short prefix stands for the full id.
    cli::context(&dir, &args(&["enqueue", &b[..8], "an ask"]), T0 + 61).unwrap();
    let shown = cli::context(&dir, &args(&["show", &b[..8]]), T0 + 62).unwrap();
    assert!(shown.contains("1 ready"), "{shown}");

    let listed = cli::context(&dir, &args(&["list"]), T0 + 62).unwrap();
    let lines: Vec<&str> = listed.lines().collect();
    assert_eq!(lines.len(), 2, "{listed}");
    assert!(lines[0].starts_with(&b[..12]), "newest first: {listed}");
    assert!(
        lines[0].contains("second") && lines[0].contains("1 ready"),
        "{listed}"
    );
    assert!(
        lines[1].starts_with(&a[..12]) && lines[1].contains("first one"),
        "{listed}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
