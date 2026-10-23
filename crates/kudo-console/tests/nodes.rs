//! A node is a second view: its governed jobs — waiting, running, recently
//! ended — and one job's output, followed, with a kill after a second tap.
//!
//! implements: 2d5dba56c923d44ea12676229e18088a19394679ab5dfcbbb25b10b08b438655@2d5dba56c923d44ea12676229e18088a19394679ab5dfcbbb25b10b08b438655

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

mod common;

use common::{focused, markup, panes};
use kudo_console::layout::THREE_PANES;
use kudo_console::model::{ConsoleData, JobOutput, JobView, NodeJobs};
use kudo_console::selection::Selection;

fn job(id: &str, state: &str, tone: &str, argv: &str, live: bool) -> JobView {
    JobView {
        id: id.into(),
        state: state.into(),
        tone: tone.into(),
        argv: argv.into(),
        cwd: "/srv".into(),
        verdict: "seed-allow".into(),
        phase: "running 3 min".into(),
        live,
    }
}

fn data() -> ConsoleData {
    ConsoleData {
        jobs: Some(NodeJobs {
            node: "pc".into(),
            jobs: vec![
                job("j-2", "pending", "warn", "nixos-rebuild switch", true),
                job("j-1", "done:0", "ok", "df -h", false),
            ],
            error: None,
        }),
        ..ConsoleData::default()
    }
}

fn render_with(
    layout: kudo_console::layout::Layout,
    data: ConsoleData,
    selection: Selection,
) -> String {
    markup(&common::render_with(layout, data, selection)).to_owned()
}

fn on_node(job: Option<&str>) -> Selection {
    let mut s = Selection::default();
    s.select_node("pc".into());
    if let Some(job) = job {
        s.select_job(job.into());
    }
    s
}

/// A node's jobs: state, argv, directory, verdict and phase each, the view
/// in focus on a phone.
#[test]
fn a_node_lists_its_jobs() {
    let html = render_with(THREE_PANES, data(), on_node(None));
    assert_eq!(panes(&html), ["where", "jobs"], "{html}");
    assert_eq!(focused(&html), Some("jobs"));
    for text in [
        "nixos-rebuild switch",
        "pending",
        "df -h",
        "done:0",
        "/srv",
        "seed-allow",
        "running 3 min",
        "data-tone=\"warn\"",
        "data-job=\"j-2\"",
    ] {
        assert!(html.contains(text), "{text:?} missing from {html}");
    }
}

/// A node whose jobs could not be read says so, rather than listing none.
#[test]
fn an_unreadable_node_says_so() {
    let html = render_with(
        THREE_PANES,
        ConsoleData {
            jobs: Some(NodeJobs {
                node: "pc".into(),
                jobs: vec![],
                error: Some("the hub did not answer".into()),
            }),
            ..ConsoleData::default()
        },
        on_node(None),
    );
    assert!(html.contains("the hub did not answer"), "{html}");
    assert!(!html.contains("no jobs on pc"), "{html}");
    let html = render_with(
        THREE_PANES,
        ConsoleData {
            jobs: Some(NodeJobs {
                node: "pc".into(),
                jobs: vec![],
                error: None,
            }),
            ..ConsoleData::default()
        },
        on_node(None),
    );
    assert!(html.contains("no jobs on pc"), "{html}");
}

/// A job's output, newest at the bottom in a view that stays at the end; a
/// running job offers a kill, an ended one does not.
#[test]
fn a_job_shows_its_output_and_a_kill_while_it_runs() {
    let mut d = data();
    d.output = Some(JobOutput {
        node: "pc".into(),
        job: "j-2".into(),
        text: "building…\ndone\n".into(),
        next_offset: 15,
        skipped: 0,
        state: "running".into(),
        tone: "ok".into(),
        live: true,
    });
    let html = render_with(THREE_PANES, d.clone(), on_node(Some("j-2")));
    assert_eq!(focused(&html), Some("job"));
    assert!(html.contains("building…"), "{html}");
    assert!(
        html.contains("data-follow"),
        "the output view follows: {html}"
    );
    assert!(html.contains(">kill</button>"), "{html}");

    let out = d.output.as_mut().unwrap();
    out.live = false;
    out.state = "done:0".into();
    out.skipped = 4096;
    let html = render_with(THREE_PANES, d, on_node(Some("j-2")));
    assert!(!html.contains(">kill</button>"), "{html}");
    assert!(html.contains("4096 bytes"), "lost output is said: {html}");
}

#[cfg(feature = "server")]
mod server_side {
    use kudo_console::views;
    use moco_job::wire::{JobSummary, TailReply};
    use moco_job::{JobStatus, Scope, Verdict};

    const NOW_MS: u64 = 1_792_500_000_000;

    fn summary(id_ms: u64, seq: u32, status: JobStatus, phase_s: u64) -> JobSummary {
        JobSummary {
            id: format!("{id_ms:013}-7-{seq}"),
            status,
            scope: Scope::System,
            name: String::new(),
            port: 0,
            external: false,
            restarts: 0,
            argv: vec!["sh".into(), "-c".into(), "echo hi there".into()],
            cwd: "/srv".into(),
            verdict: Verdict::SeedAllow,
            phase_s,
        }
    }

    /// Waiting first, then running, then ended; newest first within each.
    #[test]
    fn jobs_are_ordered_waiting_running_ended_newest_first() {
        let views = views::job_views(
            vec![
                summary(NOW_MS - 7_200_000, 1, JobStatus::Done { code: 0 }, 0),
                summary(NOW_MS - 60_000, 2, JobStatus::Running, 60),
                summary(NOW_MS - 600_000, 3, JobStatus::Running, 600),
                summary(NOW_MS - 30_000, 4, JobStatus::PendingApproval, 30),
                summary(NOW_MS - 3_600_000, 5, JobStatus::Killed, 0),
            ],
            NOW_MS,
        );
        let seqs: Vec<&str> = views
            .iter()
            .map(|v| v.id.rsplit('-').next().unwrap())
            .collect();
        assert_eq!(seqs, ["4", "2", "3", "5", "1"]);
    }

    /// Each job says its state, tone, argv (shell-quoted), verdict and phase
    /// in words a phone shows.
    #[test]
    fn a_job_reads_in_words() {
        let views = views::job_views(
            vec![
                summary(NOW_MS - 180_000, 1, JobStatus::PendingApproval, 180),
                summary(NOW_MS - 7_200_000, 2, JobStatus::Done { code: 2 }, 0),
                summary(NOW_MS - 120_000, 3, JobStatus::Running, 120),
            ],
            NOW_MS,
        );
        let by = |seq: &str| views.iter().find(|v| v.id.ends_with(seq)).unwrap();
        let waiting = by("-1");
        assert_eq!(waiting.state, "pending");
        assert_eq!(waiting.tone, "warn");
        assert_eq!(waiting.phase, "waiting 3 min");
        assert_eq!(waiting.argv, "sh -c 'echo hi there'");
        assert_eq!(
            waiting.verdict, "awaiting your verdict",
            "not the placeholder"
        );
        assert!(waiting.live);
        let failed = by("-2");
        assert_eq!(failed.tone, "bad");
        assert_eq!(failed.phase, "started 2 h ago");
        assert!(!failed.live);
        let running = by("-3");
        assert_eq!(running.tone, "ok");
        assert_eq!(running.phase, "running 2 min");
    }

    /// A job whose end no one saw says the node restarted under it.
    // implements: 0119d16e256891de4a71a14418f59c33419207f49792e58001639f8cbef567a6@d7ba7c887089648434bc819ad57473bb930242c27396d150b11617388bb85da0
    #[test]
    fn an_unknown_end_says_the_node_restarted_under_it() {
        let views = views::job_views(
            vec![summary(NOW_MS - 600_000, 1, JobStatus::OutcomeUnknown, 0)],
            NOW_MS,
        );
        assert!(views[0].phase.contains("restarted"), "{:?}", views[0].phase);
    }

    /// Output reads as text, lossily where it is not UTF-8, with what was
    /// lost and whether it still runs.
    #[test]
    fn output_reads_as_text_with_its_state() {
        let out = views::job_output(
            "pc",
            "j-1",
            &TailReply {
                bytes: b"ok\n\xff\n".to_vec(),
                next_offset: 5,
                skipped: 3,
                status: JobStatus::Running,
            },
        );
        assert!(out.text.starts_with("ok\n"), "{:?}", out.text);
        assert_eq!(out.next_offset, 5);
        assert_eq!(out.skipped, 3);
        assert!(out.live);
        assert_eq!(out.state, "running");
    }
}
