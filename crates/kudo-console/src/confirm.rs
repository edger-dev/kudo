//! A second tap before what cannot be taken back with a tap: a standing
//! verdict (it writes a rule) and a kill.
//!
//! implements: 3cebbf2e8034b89c65bb3dda9ae2be880cf6c5ba2cd928b559e7fb6dd03579f5@3cebbf2e8034b89c65bb3dda9ae2be880cf6c5ba2cd928b559e7fb6dd03579f5

/// How long an armed button waits for its second tap.
pub const CONFIRM_MS: u32 = 10_000;

/// What the second tap of `verdict` would do, for a job running `argv` on
/// `node`; `None` for a verdict that acts on the first tap.
pub fn second_tap(verdict: &str, argv: &str, node: &str) -> Option<String> {
    let word = match verdict {
        "allow-always" => "always allow",
        "deny-always" => "always deny",
        _ => return None,
    };
    Some(format!(
        "tap again to {word} exactly `{argv}` on {node} only (writes a rule)"
    ))
}

/// What a verdict's button explains on hover.
pub fn verdict_hint(verdict: &str, argv: &str, node: &str) -> String {
    second_tap(verdict, argv, node)
        .map(|t| t.replacen("tap again to ", "", 1))
        .unwrap_or_else(|| "this job only; no rule is written".to_owned())
}

/// What the second tap of a kill would do.
pub fn kill_text(job: &str, node: &str) -> String {
    format!("tap again to kill job {job} on {node}")
}
