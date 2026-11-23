use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use kudo_intake::{AdmissionRules, Note, ReceiveError, ReceiveOutcome, Receiver};

static NEXT: AtomicUsize = AtomicUsize::new(0);

fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap().trim().to_owned()
}

fn repo(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "kudo-intake-receiver-{}-{}-{name}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&path);
    std::fs::create_dir_all(&path).unwrap();
    let init = Command::new("git")
        .args(["init", "-b", "main"])
        .arg(&path)
        .output()
        .unwrap();
    assert!(init.status.success());
    git(&path, &["config", "user.email", "kudo@example.org"]);
    git(&path, &["config", "user.name", "Kudo Test"]);
    kinora::init::init(&path, "https://example.com/intake.git").unwrap();
    let root = path.join(".kinora");
    let config_path = root.join("config.styx");
    let text = std::fs::read_to_string(&config_path).unwrap();
    let mut config = kinora::config::Config::from_styx(&text).unwrap();
    config
        .roots
        .insert("inbox".into(), kinora::config::RootPolicy::Never);
    config
        .roots
        .insert("specs".into(), kinora::config::RootPolicy::Never);
    std::fs::write(&config_path, config.to_styx().unwrap()).unwrap();
    git(&path, &["add", ".kinora"]);
    git(&path, &["commit", "-m", "init"]);
    path
}

fn receiver(repo: &Path) -> Receiver {
    Receiver::new(BTreeMap::from([("kudo".into(), repo.to_path_buf())]))
}

fn note(id: &str) -> Note {
    Note {
        id: id.into(),
        target: "kudo".into(),
        title: "A design thought".into(),
        body: "The receiver should stay thin.".into(),
        attestation: true,
        author: "YJ Park".into(),
        machine: "work".into(),
        session: "session-123".into(),
        created_at: "2026-11-23T12:00:00Z".into(),
    }
}

fn rules() -> AdmissionRules {
    AdmissionRules {
        max_body_bytes: 1024,
        sensitive_markers: vec![],
    }
}

fn delivered(receiver: &Receiver, note: &Note, received_at: &str) -> String {
    match receiver.receive(note, &rules(), received_at).unwrap() {
        ReceiveOutcome::Delivered { kino_id } => kino_id,
        other => panic!("expected delivery, got {other:?}"),
    }
}

#[test]
fn a_note_commits_with_provenance_and_lists_by_receipt_time() {
    let repo = repo("delivery");
    let receiver = receiver(&repo);
    let before = git(&repo, &["rev-list", "--count", "HEAD"]);
    let kino_id = delivered(&receiver, &note("note-1"), "2026-11-23T12:01:00Z");
    assert!(!kino_id.is_empty());
    assert_eq!(
        git(&repo, &["rev-list", "--count", "HEAD"])
            .parse::<usize>()
            .unwrap(),
        before.parse::<usize>().unwrap() + 1
    );

    let listed = receiver.list_since("kudo", "2026-11-23T12:00:00Z").unwrap();
    assert_eq!(listed.len(), 1);
    let arrival = &listed[0];
    assert_eq!(arrival.note_id, "note-1");
    assert_eq!(arrival.kino_id, kino_id);
    assert_eq!(arrival.title, "A design thought");
    assert_eq!(arrival.created_at, "2026-11-23T12:00:00Z");
    assert_eq!(arrival.received_at, "2026-11-23T12:01:00Z");
    assert!(arrival.unreviewed);
    assert!(
        receiver
            .list_since("kudo", "2026-11-23T12:02:00Z")
            .unwrap()
            .is_empty()
    );
}

#[test]
fn retries_deduplicate_and_changed_content_conflicts() {
    let repo = repo("retry");
    let receiver = receiver(&repo);
    let kino_id = delivered(&receiver, &note("note-1"), "2026-11-23T12:01:00Z");
    let commits = git(&repo, &["rev-list", "--count", "HEAD"]);
    assert_eq!(
        receiver
            .receive(&note("note-1"), &rules(), "2026-11-23T12:01:00Z")
            .unwrap(),
        ReceiveOutcome::Duplicate {
            kino_id: kino_id.clone()
        }
    );
    assert_eq!(git(&repo, &["rev-list", "--count", "HEAD"]), commits);

    let mut changed = note("note-1");
    changed.body = "Different content".into();
    assert!(matches!(
        receiver.receive(&changed, &rules(), "2026-11-23T12:02:00Z"),
        Err(ReceiveError::IdConflict)
    ));
}

#[test]
fn deduplication_survives_triage_out_of_the_inbox() {
    let repo = repo("triage");
    let receiver = receiver(&repo);
    let kino_id = delivered(&receiver, &note("note-1"), "2026-11-23T12:01:00Z");
    let root = repo.join(".kinora");
    kinora::assign::write_assign(
        &root,
        &kinora::assign::AssignEvent {
            kino_id: kino_id.clone(),
            target_root: "specs".into(),
            supersedes: vec![],
            author: "YJ Park".into(),
            ts: "2026-11-23T12:02:00Z".into(),
            provenance: "test-triage".into(),
        },
    )
    .unwrap();
    kinora::commit::commit_all(
        &root,
        kinora::commit::CommitParams {
            author: "YJ Park".into(),
            ts: "2026-11-23T12:02:00Z".into(),
            provenance: "test-triage".into(),
        },
    )
    .unwrap();
    git(&repo, &["add", ".kinora"]);
    git(&repo, &["commit", "-m", "triage"]);

    assert_eq!(
        receiver
            .receive(&note("note-1"), &rules(), "2026-11-23T12:03:00Z")
            .unwrap(),
        ReceiveOutcome::Duplicate { kino_id }
    );
}

#[test]
fn unknown_aliases_and_unready_checkouts_write_nothing() {
    let repo = repo("refusal");
    let receiver = receiver(&repo);
    assert!(matches!(
        receiver.receive(
            &Note {
                target: "unknown".into(),
                ..note("note-1")
            },
            &rules(),
            "2026-11-23T12:01:00Z"
        ),
        Err(ReceiveError::UnknownTarget(_))
    ));

    std::fs::write(repo.join("unrelated"), "dirty").unwrap();
    assert!(matches!(
        receiver.receive(&note("note-1"), &rules(), "2026-11-23T12:01:00Z"),
        Err(ReceiveError::CheckoutNotReady(_))
    ));
    std::fs::remove_file(repo.join("unrelated")).unwrap();
    git(&repo, &["checkout", "--detach"]);
    assert!(matches!(
        receiver.receive(&note("note-1"), &rules(), "2026-11-23T12:01:00Z"),
        Err(ReceiveError::CheckoutNotReady(_))
    ));
}

#[test]
fn arrivals_are_filtered_and_sorted_by_receipt_time() {
    let repo = repo("listing");
    let receiver = receiver(&repo);
    delivered(&receiver, &note("note-1"), "2026-11-23T12:01:00Z");
    delivered(&receiver, &note("note-2"), "2026-11-23T12:03:00Z");
    delivered(&receiver, &note("note-3"), "2026-11-23T12:02:00Z");
    let listed = receiver.list_since("kudo", "2026-11-23T12:02:00Z").unwrap();
    let ids: Vec<_> = listed.iter().map(|note| note.note_id.as_str()).collect();
    assert_eq!(ids, ["note-3", "note-2"]);
}

#[test]
fn timestamps_must_be_rfc3339() {
    let repo = repo("time");
    let receiver = receiver(&repo);
    let mut invalid = note("note-1");
    invalid.created_at = "yesterday".into();
    assert!(matches!(
        receiver.receive(&invalid, &rules(), "2026-11-23T12:01:00Z"),
        Err(ReceiveError::InvalidTime(_))
    ));
    assert!(matches!(
        receiver.receive(&note("note-1"), &rules(), "now"),
        Err(ReceiveError::InvalidTime(_))
    ));
    assert!(matches!(
        receiver.list_since("kudo", "yesterday"),
        Err(ReceiveError::InvalidTime(_))
    ));
}
