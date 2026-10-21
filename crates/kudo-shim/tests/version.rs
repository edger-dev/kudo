//! `kudo-shim --version` and `--help` answer at once; no MCP server starts.
//!
//! implements: 35e0ccd93acf733faaacad05140d9932384b173b1a02353b7145c879441b2c6a@35e0ccd93acf733faaacad05140d9932384b173b1a02353b7145c879441b2c6a

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::process::{Command, Stdio};

fn run(arg: &str) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_kudo-shim"))
        .arg(arg)
        .stdin(Stdio::null())
        .env_remove("KUDO_HUB_ADDR")
        .output()
        .unwrap();
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn version_prints_the_version() {
    for arg in ["--version", "-V", "version"] {
        let (ok, out) = run(arg);
        assert!(ok, "{arg}: {out}");
        assert_eq!(
            out.trim(),
            format!("kudo-shim {}", env!("CARGO_PKG_VERSION")),
            "{arg}"
        );
    }
}

#[test]
fn help_names_every_face() {
    for arg in ["--help", "-h", "help"] {
        let (ok, out) = run(arg);
        assert!(ok, "{arg}: {out}");
        for face in ["hook", "context", "worker", "MCP"] {
            assert!(out.contains(face), "{arg} names {face}: {out}");
        }
    }
}
