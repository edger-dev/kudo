//! `kudo-shim hook <event>`: the agent CLI's hooks, answered from the context
//! store with no hub and no node on the path — a worker survives both
//! restarting.
//!
//! The CLI runs a hook as a command, feeds it the event as JSON on stdin and
//! reads its decision from stdout. Each event maps to one worker decision:
//!
//! | event                | answer                                             |
//! |----------------------|----------------------------------------------------|
//! | `session-start`      | the head and the worker's rules as added context   |
//! | `stop`               | `{"decision":"block","reason":…}`, or nothing      |
//! | `pre-compact`        | ends the lease                                     |
//! | `user-prompt-submit` | ends the lease unless it is the launcher's kickoff or the CLI's own task notification |
//!
//! implements: b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af@fcfe5ccb3d93596f43b2613510e6683f314db40859230f60087eb334dea04439

use kudo_context::Now;
use kudo_worker::{LeaseBook, Stop, Worker, WorkerSetup};
use serde_json::{Value, json};

/// What a hook prints and how it exits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookReply {
    pub stdout: String,
    pub code: i32,
}

impl HookReply {
    fn quiet() -> Self {
        Self {
            stdout: String::new(),
            code: 0,
        }
    }

    fn json(value: &Value) -> Self {
        Self {
            stdout: value.to_string(),
            code: 0,
        }
    }

    /// A failure the CLI shows the human (exit code 1 is non-blocking: the
    /// session goes on, but the error is visible).
    fn failed(message: impl std::fmt::Display) -> Self {
        Self {
            stdout: String::new(),
            code: 1,
        }
        .with_stderr(message)
    }

    fn with_stderr(self, message: impl std::fmt::Display) -> Self {
        eprintln!("kudo-shim hook: {message}");
        self
    }
}

/// Answer one hook event for the worker `setup` describes.
pub fn run(event: &str, stdin: &str, setup: &WorkerSetup, now: Now) -> HookReply {
    run_with(event, stdin, setup, now, crate::events::observer_from_env())
}

/// [`run`], with the observer of the loop's events given.
pub fn run_with(
    event: &str,
    stdin: &str,
    setup: &WorkerSetup,
    now: Now,
    observer: Option<kudo_worker::Observer>,
) -> HookReply {
    let input: Value = serde_json::from_str(stdin).unwrap_or(Value::Null);
    // A shell call's record needs no mirror, and must not wait on one.
    // implements: 6061a14e7f9b9eb2c7a3bbc0f5ff4f3778ceb08d71d46f8de4515f62ef3a2a0b@fdf56a326ad7b945b7ee31bb9503ee3365f629c5bf46347c3a8c970398fa65bd
    if matches!(event, "post-tool-use" | "post-tool-use-failure") {
        return post_tool_use(&input, setup, now);
    }
    let store = match crate::cli::open_store(
        &setup.store_dir,
        crate::cli::mirror_from_env().as_deref(),
        false,
    ) {
        Ok(store) => store,
        Err(e) => return HookReply::failed(e),
    };
    // A waiting Stop looks every few seconds: its mirror syncs at a wider
    // spacing (claims and submits still force theirs).
    let store = if event == "stop" {
        store.with_sync_spacing(std::time::Duration::from_secs(kudo_worker::env::WAIT_SYNC))
    } else {
        store
    };
    let book = LeaseBook::new(&setup.leases_dir());
    let mut worker = Worker::new(&store, &book, setup.config());
    if let Some(observer) = observer {
        worker = worker.with_observer(observer);
    }

    let result = match event {
        "session-start" => worker.session_start(now).map(|text| {
            HookReply::json(&json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": text,
                }
            }))
        }),
        // Waits for work: the clock runs on from `now`.
        "stop" => {
            let started = std::time::Instant::now();
            let clock = || now + i64::try_from(started.elapsed().as_secs()).unwrap_or(0);
            let pause =
                || std::thread::sleep(std::time::Duration::from_secs(kudo_worker::env::WAIT_POLL));
            worker.stop_waiting(clock, pause, kudo_worker::env::WAIT_WINDOW)
        }
        .map(|stop| match stop {
            Stop::Block { reason } => HookReply::json(&json!({
                "decision": "block",
                "reason": reason,
            })),
            Stop::Allow(_) | Stop::Wait => HookReply::quiet(),
        }),
        "pre-compact" => worker.pre_compact(now).map(|()| HookReply::quiet()),
        "user-prompt-submit" => {
            let prompt = input
                .get("prompt")
                .and_then(Value::as_str)
                .unwrap_or_default();
            worker.user_prompt(prompt, now).map(|()| HookReply::quiet())
        }
        other => return HookReply::failed(format!("no hook named `{other}`")),
    };
    result.unwrap_or_else(HookReply::failed)
}

/// A shell call just returned (or failed): record it in the lease's trace
/// and tell the worker the kino to cite. Any other tool is let be. Best
/// effort: a failed record never fails the call.
fn post_tool_use(input: &Value, setup: &WorkerSetup, now: Now) -> HookReply {
    if input.get("tool_name").and_then(Value::as_str) != Some("Bash") {
        return HookReply::quiet();
    }
    let command = input
        .pointer("/tool_input/command")
        .and_then(Value::as_str)
        .unwrap_or_default();
    // A call that runs this hook itself would write a record no tool call
    // made: it is not recorded (the call that ran it is, and shows it).
    if command.contains("hook post-tool-use") {
        return HookReply::quiet();
    }
    let response = input.get("tool_response").cloned().unwrap_or(Value::Null);
    let text = |key: &str| {
        response
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
    };
    let mut output = match response.as_str() {
        Some(whole) => whole.to_owned(),
        None => text("stdout").to_owned(),
    };
    if !text("stderr").is_empty() {
        output.push_str("\n[stderr]\n");
        output.push_str(text("stderr"));
    }
    // A failed call (PostToolUseFailure) carries its error instead.
    if let Some(error) = input.get("error").and_then(Value::as_str) {
        output.push_str("\n[error]\n");
        output.push_str(error);
    }
    let exit = ["exit_code", "exitCode", "returnCode", "code"]
        .iter()
        .find_map(|k| response.get(*k).and_then(Value::as_i64))
        .and_then(|c| i32::try_from(c).ok());
    // Written locally only: a shell call must not wait on the network; the
    // next sync (the Stop hook, a submit) carries it to the mirror.
    let store = match crate::cli::open_store(&setup.store_dir, None, false) {
        Ok(store) => store,
        Err(e) => {
            eprintln!("kudo: could not record the shell call: {e}");
            return HookReply::quiet();
        }
    };
    match store.record_tool_call(&setup.context, &setup.lease, command, exit, &output, now) {
        Ok(id) => HookReply::json(&json!({
            "hookSpecificOutput": {
                "hookEventName": input
                    .get("hook_event_name")
                    .and_then(Value::as_str)
                    .unwrap_or("PostToolUse"),
                "additionalContext": format!(
                    "kudo recorded this shell call as kino://{id}: cite it as evidence for \
                     what it showed."
                ),
            }
        })),
        Err(e) => {
            eprintln!("kudo: could not record the shell call: {e}");
            HookReply::quiet()
        }
    }
}
