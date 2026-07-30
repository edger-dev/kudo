//! Architectural invariants enforced as plain tests — the mechanism for
//! structural rules that clippy cannot express. Drop this file into a crate's
//! `tests/` directory; it walks that crate's `src/` and asserts the invariant,
//! failing as a red `cargo test` in the normal TDD loop.
//!
//! implements: jig::rust::prefer-file-modules

use std::path::{Path, PathBuf};

/// Recursively collect every file named `mod.rs` under `dir`.
fn find_mod_rs(dir: &Path, hits: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_mod_rs(&path, hits);
        } else if path.file_name().is_some_and(|name| name == "mod.rs") {
            hits.push(path);
        }
    }
}

/// rule: jig::rust::prefer-file-modules — prefer `foo.rs` over `foo/mod.rs`.
#[test]
fn no_mod_rs_files() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut hits = Vec::new();
    find_mod_rs(&src, &mut hits);
    assert!(
        hits.is_empty(),
        "rule jig::rust::prefer-file-modules: use `foo.rs` beside `foo/`, not \
         `foo/mod.rs`. Offending files: {hits:#?}"
    );
}

/// **The daemon must die alone.**
///
/// systemd's default `KillMode=control-group` signals every process in the
/// cgroup, which is every job this daemon supervises — so restarting the
/// supervisor would take down the servers and builds it exists to keep alive,
/// and "a job outlives any one client" would be false exactly when it matters.
///
/// Asserted here because the unit file is the kind of thing that gets tidied
/// back to a default by someone who does not know what it is load-bearing for.
///
/// implements: the-daemon-dies-alone
#[test]
#[allow(clippy::expect_used, reason = "a failure here is a broken harness")]
fn the_systemd_unit_does_not_kill_the_jobs_with_the_daemon() {
    let unit = include_str!("../packaging/kudo-node.service");

    let kill_mode: Vec<&str> = unit
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with("KillMode="))
        .collect();

    assert_eq!(
        kill_mode,
        vec!["KillMode=process"],
        "the unit must set KillMode=process exactly once — the default signals \
         every supervised job along with the daemon"
    );
}

/// The unit names the PTY holder, and names it somewhere the daemon can find.
///
/// A terminal job runs *as* the holder, so this line is what lets such a job
/// keep its terminal across a restart of the unit. Its absence is not fatal —
/// the engine treats durability as opt-in — which is exactly why it needs a
/// test: a silently missing holder looks like nothing at all until someone
/// restarts the daemon and loses a terminal they were watching.
///
/// implements: the-daemon-dies-alone
#[test]
#[allow(clippy::expect_used, reason = "a failure here is a broken harness")]
fn the_systemd_unit_points_at_the_pty_holder() {
    let unit = include_str!("../packaging/kudo-node.service");
    assert!(
        unit.lines()
            .map(str::trim)
            .any(|l| l.starts_with("Environment=KUDO_PTY_HOLDER=")),
        "the unit must name the holder binary, or terminal jobs quietly lose \
         their terminal on every restart"
    );
}
