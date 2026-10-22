//! The approval queue: every node's jobs waiting for the author's verdict,
//! oldest first, with what is needed to answer — and the four verdicts, plus
//! edit-then-allow, a tap away.
//!
//! implements: kudo:daf63591a53811da5e761c847a528776758b90ee07fc55f8c16ce14789be1f44

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use dioxus::prelude::*;
use kudo_console::layout::THREE_PANES;
use kudo_console::model::{ApprovalView, ConsoleData};
use kudo_console::shell::Shell;

fn waiting(node: &str, job: &str, argv: &[&str], waited_s: u64) -> ApprovalView {
    ApprovalView {
        node: node.into(),
        job: job.into(),
        argv: argv.iter().map(|a| (*a).to_owned()).collect(),
        cwd: "/tmp".into(),
        owner: "/home/yj/agents/edger/kudo".into(),
        waited_s,
        left_s: 1800 - waited_s,
        context: Some("home lab".into()),
    }
}

fn render(data: ConsoleData) -> String {
    dioxus_ssr::render_element(rsx! { Shell { layout: THREE_PANES, data: data } })
}

/// A waiting job shows its node, argv, where it runs, who asked and the time
/// left, with the verdict buttons.
#[test]
fn a_waiting_job_shows_what_is_needed_to_answer_it() {
    let html = render(ConsoleData {
        approvals: vec![waiting("pc", "j-1", &["df", "-h", "/"], 60)],
        ..ConsoleData::default()
    });
    for text in [
        "waiting for you",
        "df -h /",
        "pc",
        "/tmp",
        "home lab",
        "29 min left",
        "allow once",
        "allow always",
        "deny once",
        "deny always",
        "edit, then allow",
    ] {
        assert!(html.contains(text), "{text:?} missing from {html}");
    }
}

/// Nothing waiting, no queue.
#[test]
fn no_waiting_jobs_no_queue() {
    let html = render(ConsoleData::default());
    assert!(!html.contains("waiting for you"));
}

#[cfg(feature = "server")]
mod server_side {
    use std::collections::HashMap;

    use kudo_console::views;
    use kudo_context::{ContextStore, TraceEvent};
    use moco_job::Scope;
    use moco_job::wire::{PendingJob, WireVerdict};

    const T0: i64 = 1_792_500_000;

    fn pending(id: &str, waited_ms: u64) -> PendingJob {
        PendingJob {
            id: id.into(),
            argv: vec!["df".into(), "-h".into()],
            cwd: "/tmp".into(),
            scope: Scope::System,
            waited_ms,
            left_ms: 1_800_000 - waited_ms,
        }
    }

    /// Every node's waiting jobs, oldest first, each named by the context
    /// whose worker asked (when one traced it).
    #[test]
    fn approvals_are_oldest_first_and_name_the_context_that_asked() {
        let mut asked = HashMap::new();
        asked.insert(("pc".to_owned(), "j-old".to_owned()), "home lab".to_owned());
        let views = views::approvals(
            vec![
                ("yolo".into(), pending("j-new", 1_000)),
                ("pc".into(), pending("j-old", 90_000)),
            ],
            &asked,
        );
        let order: Vec<&str> = views.iter().map(|v| v.job.as_str()).collect();
        assert_eq!(order, vec!["j-old", "j-new"]);
        assert_eq!(views[0].node, "pc");
        assert_eq!(views[0].context.as_deref(), Some("home lab"));
        assert_eq!(views[0].waited_s, 90);
        assert_eq!(views[1].context, None);
    }

    /// Which context's worker started a job, from the job-start events.
    #[test]
    fn the_context_that_asked_is_read_from_the_trace() {
        let dir = std::env::temp_dir().join(format!("kudo-console-asked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = ContextStore::init(&dir).unwrap();
        let ctx = store
            .create_context("home lab", "# home lab\n", T0)
            .unwrap();
        store
            .record_trace(
                &ctx,
                "lease-1",
                &TraceEvent::JobStart {
                    node: "pc".into(),
                    job: "j-1".into(),
                    waiting: true,
                    argv: "df -h".into(),
                },
                T0,
            )
            .unwrap();
        let asked = views::job_contexts(&store).unwrap();
        assert_eq!(
            asked
                .get(&("pc".to_owned(), "j-1".to_owned()))
                .map(String::as_str),
            Some("home lab")
        );
    }

    /// The verdict names the buttons send.
    #[test]
    fn verdict_names_map_to_the_four_verdicts() {
        assert_eq!(
            views::verdict_from("allow-once"),
            Some(WireVerdict::AllowOnce)
        );
        assert_eq!(
            views::verdict_from("allow-always"),
            Some(WireVerdict::AllowAlways)
        );
        assert_eq!(
            views::verdict_from("deny-once"),
            Some(WireVerdict::DenyOnce)
        );
        assert_eq!(
            views::verdict_from("deny-always"),
            Some(WireVerdict::DenyAlways)
        );
        assert_eq!(views::verdict_from("maybe"), None);
    }
}
