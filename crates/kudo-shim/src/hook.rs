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
//! | `user-prompt-submit` | ends the lease unless it is the launcher's kickoff |
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
    let _ = observer;
    let input: Value = serde_json::from_str(stdin).unwrap_or(Value::Null);
    let store = match crate::cli::open_store(
        &setup.store_dir,
        crate::cli::mirror_from_env().as_deref(),
        false,
    ) {
        Ok(store) => store,
        Err(e) => return HookReply::failed(e),
    };
    let book = LeaseBook::new(&setup.leases_dir());
    let worker = Worker::new(&store, &book, setup.config());

    let result = match event {
        "session-start" => worker.session_start(now).map(|text| {
            HookReply::json(&json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": text,
                }
            }))
        }),
        "stop" => worker.stop(now).map(|stop| match stop {
            Stop::Block { reason } => HookReply::json(&json!({
                "decision": "block",
                "reason": reason,
            })),
            Stop::Allow(_) => HookReply::quiet(),
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
