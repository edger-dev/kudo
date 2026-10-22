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
        "on pc in /tmp",
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
        asked.insert("j-old".to_owned(), "home lab".to_owned());
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
        let name = asked.get("j-1").expect("the job is named by its context");
        assert!(name.starts_with("home lab "), "{name}");
        assert!(
            name.ends_with(&ctx.0[..12]),
            "titles need not be unique: {name}"
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

    /// Every button sends a verdict the server reads.
    #[test]
    fn every_button_sends_a_known_verdict() {
        for (label, verdict) in kudo_console::shell::VERDICTS {
            assert!(
                views::verdict_from(verdict).is_some(),
                "{label} sends {verdict}"
            );
        }
        assert_eq!(kudo_console::shell::VERDICTS.len(), 4);
    }
}

/// An argv reads and edits as one shell-quoted line: an element with a space
/// stays one element, and an unchanged edit is the same argv.
#[test]
fn an_argv_round_trips_through_its_text() {
    use kudo_console::model::{argv_parse, argv_text};
    let argv: Vec<String> = ["sh", "-c", "rm -rf /tmp/x", ""]
        .iter()
        .map(|a| (*a).to_owned())
        .collect();
    let text = argv_text(&argv);
    assert_ne!(text, "sh -c rm -rf /tmp/x ", "quoted, not joined: {text}");
    assert_eq!(argv_parse(&text), Some(argv));
    assert_eq!(argv_parse("   "), None, "an empty edit is refused");
    assert_eq!(argv_parse("echo 'unclosed"), None);
}

/// A node whose queue could not be read says so.
#[test]
fn an_unreadable_queue_is_said() {
    let html = render(ConsoleData {
        approvals_unknown: vec!["edger".into()],
        ..ConsoleData::default()
    });
    assert!(html.contains("queue unknown on edger"), "{html}");
}
