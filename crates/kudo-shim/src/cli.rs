//! The shim's local commands: create and drive a context, and start a worker
//! in this terminal. A thin face over the context engine.

use std::path::Path;

/// Run `kudo-shim context <args…>` against the store in `store_dir`; the text
/// to print, or an error.
pub fn context(_store_dir: &Path, _args: &[String], _now: i64) -> Result<String, String> {
    Err("not built yet".into())
}
