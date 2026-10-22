//! What crosses from the console server to the page: plain data, nothing a
//! renderer must compute.

use serde::{Deserialize, Serialize};

/// A node the hub reaches, and its links.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeView {
    pub id: String,
    pub links: Vec<String>,
    /// Whether the console may start a worker here.
    pub may_start: bool,
}

/// A lease still running under a context, and the last thing it did.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkerView {
    pub lease: String,
    pub last: String,
    /// Silent for longer than a claim lives: likely dead, not working.
    pub quiet: bool,
    /// The pane it runs in, when the console started it.
    pub pane: Option<PaneRef>,
}

/// A pane on a node: the job that is a worker's terminal.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PaneRef {
    pub node: String,
    pub job: String,
}

/// The pane being taken over, and what it shows.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TerminalView {
    pub pane: PaneRef,
    pub screen: String,
}

/// A context as the *where* pane lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextCard {
    pub id: String,
    pub title: String,
    pub ready: usize,
    pub submitted: usize,
    /// Items not yet due, and when the next one is.
    pub later: usize,
    pub next_due: Option<String>,
    pub workers: Vec<WorkerView>,
}

/// One trace event, as the trace pane shows it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceRow {
    pub at: String,
    pub lease: String,
    pub seq: u64,
    pub line: String,
}

/// The selected context: its head and its trace.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDetail {
    pub id: String,
    pub title: String,
    pub head: String,
    pub forked: bool,
    pub trace: Vec<TraceRow>,
}

/// Everything the panes draw from.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsoleData {
    pub nodes: Vec<NodeView>,
    pub contexts: Vec<ContextCard>,
    pub selected: Option<ContextDetail>,
    /// What went wrong reading the selection, said beside the panes.
    pub error: Option<String>,
    /// Jobs on any node waiting for the author's verdict, oldest first.
    pub approvals: Vec<ApprovalView>,
    /// Nodes whose queue could not be read: nothing shown there is not
    /// "nothing waiting".
    pub approvals_unknown: Vec<String>,
    /// The pane being taken over, if any.
    pub terminal: Option<TerminalView>,
}

/// A job waiting for the author's verdict.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalView {
    pub node: String,
    pub job: String,
    pub argv: Vec<String>,
    pub cwd: String,
    /// The workspace that owns it.
    pub owner: String,
    pub waited_s: u64,
    pub left_s: u64,
    /// The context whose worker asked, when one traced it.
    pub context: Option<String>,
}

/// An argv as one line a person reads and edits: shell-quoted, so
/// `["sh", "-c", "rm -rf x"]` never reads like `sh -c rm -rf x`.
pub fn argv_text(argv: &[String]) -> String {
    shlex::try_join(argv.iter().map(String::as_str)).unwrap_or_else(|_| format!("{argv:?}"))
}

/// An edited line back to an argv: `None` when it does not parse or is empty
/// — an edit never quietly becomes the original command.
pub fn argv_parse(line: &str) -> Option<Vec<String>> {
    shlex::split(line).filter(|argv| !argv.is_empty())
}
