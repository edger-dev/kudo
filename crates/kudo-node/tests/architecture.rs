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

/// The unit gives the daemon a **stable** state directory.
///
/// The engine's default is a per-process directory — correct for a library and
/// ruinous for a daemon: a supervisor handed a fresh directory on every start
/// re-adopts nothing, boots nothing, and forgets every port it ever allocated.
/// Every durability feature in the system is silently inert without this, and
/// nothing fails loudly to say so.
///
/// implements: the-daemon-dies-alone
#[test]
#[allow(clippy::expect_used, reason = "a failure here is a broken harness")]
fn the_systemd_unit_gives_the_daemon_a_stable_state_directory() {
    let unit = include_str!("../packaging/kudo-node.service");
    assert!(
        unit.lines()
            .map(str::trim)
            .any(|l| l.starts_with("Environment=MOCO_DIR=")),
        "without a stable state directory every durability feature is inert"
    );
}

/// The home-manager module that deploys the daemon as a user service.
///
/// Read at run time rather than with `include_str!`, so a missing module is a
/// red test, not a build failure.
fn home_module() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("packaging/home-module.nix");
    std::fs::read_to_string(&path).unwrap_or_default()
}

/// The home-manager module carries the same three load-bearing lines as the
/// unit file: it is the same deployment written a second way, and the module
/// is what actually runs on the nodes.
///
/// implements: the-daemon-dies-alone
#[test]
fn the_home_module_keeps_the_three_load_bearing_lines() {
    let module = home_module();
    for (needle, why) in [
        (
            r#"KillMode = "process""#,
            "the default kill mode takes every supervised job down with the daemon",
        ),
        (
            "KUDO_PTY_HOLDER=",
            "without the holder, terminal jobs lose their terminal on restart",
        ),
        (
            "MOCO_DIR=",
            "without a stable state directory every durability feature is inert",
        ),
    ] {
        assert!(
            module.contains(needle),
            "packaging/home-module.nix must contain `{needle}`: {why}"
        );
    }
}
