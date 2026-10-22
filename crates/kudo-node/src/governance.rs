//! The node's governance, from its configuration: every node runs governed.
//!
//! The policy is a deployment fact — which rules this machine pre-allows, the
//! root its jobs run under, how long a request waits for the author — so the
//! composition root reads it from the environment its unit is given, and the
//! engine applies it.
//!
//! implements: kudo:daf63591a53811da5e761c847a528776758b90ee07fc55f8c16ce14789be1f44

use std::path::PathBuf;
use std::time::Duration;

use moco_job::{NodePolicy, RuleSet};

/// A Styx seed file (`allow (…)` / `deny (…)`, exact argvs). Unset: no rules,
/// so every command waits for a verdict.
pub const RULES_ENV: &str = "KUDO_NODE_RULES";
/// The directory every job's cwd must resolve inside (default `/`: the gate is
/// the exact argv and the human, not the directory).
pub const ALLOWED_ROOT_ENV: &str = "KUDO_NODE_ALLOWED_ROOT";
/// Seconds a request waits for a verdict before it is denied.
pub const APPROVAL_TIMEOUT_ENV: &str = "KUDO_APPROVAL_TIMEOUT";
/// Room to see a request on a phone.
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(30 * 60);

/// The policy `lookup` (the environment) describes. A rules file that is
/// named but missing or broken is an error naming it: a node must not come up
/// with its rules silently gone.
pub fn policy_from(lookup: impl Fn(&str) -> Option<String>) -> Result<NodePolicy, String> {
    let rules = match lookup(RULES_ENV).filter(|p| !p.is_empty()) {
        None => RuleSet::default(),
        Some(path) => {
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("{RULES_ENV}: cannot read {path}: {e}"))?;
            RuleSet::from_styx(&text).map_err(|e| format!("{RULES_ENV}: {path}: {e}"))?
        }
    };
    let root = lookup(ALLOWED_ROOT_ENV)
        .filter(|r| !r.is_empty())
        .map_or_else(|| PathBuf::from("/"), PathBuf::from);
    let timeout = match lookup(APPROVAL_TIMEOUT_ENV).filter(|t| !t.is_empty()) {
        None => DEFAULT_APPROVAL_TIMEOUT,
        Some(t) => match t.parse::<u64>() {
            Ok(secs) if secs > 0 => Duration::from_secs(secs),
            _ => {
                return Err(format!(
                    "{APPROVAL_TIMEOUT_ENV}: {t:?} is not a positive number of seconds"
                ));
            }
        },
    };
    Ok(NodePolicy::new(rules, root).with_approval_timeout(timeout))
}
