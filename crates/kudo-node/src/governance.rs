//! The node's governance, from its configuration.
use std::time::Duration;

use moco_job::NodePolicy;

pub const RULES_ENV: &str = "KUDO_NODE_RULES";
pub const ALLOWED_ROOT_ENV: &str = "KUDO_NODE_ALLOWED_ROOT";
pub const APPROVAL_TIMEOUT_ENV: &str = "KUDO_APPROVAL_TIMEOUT";
pub const DEFAULT_APPROVAL_TIMEOUT: Duration = Duration::from_secs(1);

pub fn policy_from(_lookup: impl Fn(&str) -> Option<String>) -> Result<NodePolicy, String> {
    Err("not yet".into())
}
