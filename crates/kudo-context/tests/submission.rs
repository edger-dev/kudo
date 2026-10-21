//! The typed submission: an established line keeps its place only with
//! evidence that resolves; otherwise it lands as claimed. The body reads cold,
//! in fixed sections.
//!
//! implements: 4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9@4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_context::{Claim, ContextId, ContextStore, Established, RuledOut, Submission};

const T0: i64 = 1_792_500_000;

fn claimed(name: &str) -> (std::path::PathBuf, ContextStore, ContextId, Claim) {
    let dir = std::env::temp_dir().join(format!("kudo-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let ctx = store.create_context("typed", "# typed\n", T0).unwrap();
    store.enqueue(&ctx, "find out", T0).unwrap();
    let claim = store.pull(&ctx, "lease-a", T0, 600).unwrap().unwrap();
    (dir, store, ctx, claim)
}

fn est(claim: &str, evidence: &[&str]) -> Established {
    Established {
        claim: claim.into(),
        evidence: evidence.iter().map(|e| e.to_string()).collect(),
    }
}

#[test]
fn evidence_that_resolves_keeps_a_line_established() {
    let (dir, store, ctx, claim) = claimed("resolves");
    let sub = Submission {
        head: "# typed\nfound\n".into(),
        log: "looked into it".into(),
        established: vec![est("the docs say so", &["https://example.org/docs#x"])],
        ..Default::default()
    };
    let first = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert!(first.downgraded.is_empty(), "{first:?}");
    assert!(first.unresolved.is_empty(), "{first:?}");

    // An earlier submission is evidence too: what it established, with its own.
    store.enqueue(&ctx, "build on it", T0 + 2).unwrap();
    let next = store.pull(&ctx, "lease-a", T0 + 2, 600).unwrap().unwrap();
    let sub = Submission {
        head: "# typed
built on it
"
        .into(),
        log: "built on it".into(),
        established: vec![est("the first submit found it", &[&first.id.0])],
        ..Default::default()
    };
    let second = store.submit(&next.id, &sub, T0 + 3).unwrap();
    assert!(second.downgraded.is_empty(), "{second:?}");

    let log = store.log(&ctx).unwrap();
    let established = section(&log[0], "Established");
    assert!(established.contains("the docs say so"), "{log:?}");
    assert!(
        established.contains("https://example.org/docs#x"),
        "{log:?}"
    );
    assert!(
        section(&log[1], "Established").contains(&first.id.0),
        "{log:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_line_without_evidence_that_resolves_lands_as_claimed() {
    let (dir, store, ctx, claim) = claimed("downgrade");
    let missing = "ab".repeat(32);
    let sub = Submission {
        head: "# typed\nmaybe\n".into(),
        log: "guessed".into(),
        established: vec![
            est("no evidence at all", &[]),
            est("a kino nobody stored", &[&missing]),
            est("a span before transcripts exist", &["transcript:12-30"]),
        ],
        claimed: vec!["said as a claim".into()],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert_eq!(
        done.downgraded,
        [
            "no evidence at all",
            "a kino nobody stored",
            "a span before transcripts exist"
        ]
    );
    assert!(done.unresolved.contains(&missing), "{done:?}");

    let body = &store.log(&ctx).unwrap()[0];
    assert!(
        !section(body, "Established").contains("no evidence at all"),
        "{body}"
    );
    let claimed = section(body, "Claimed");
    for line in [
        "no evidence at all",
        "a kino nobody stored",
        "said as a claim",
    ] {
        assert!(claimed.contains(line), "{line} in {body}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_line_with_some_evidence_that_resolves_stays_and_the_rest_is_reported() {
    let (dir, store, ctx, claim) = claimed("mixed");
    let missing = "cd".repeat(32);
    let sub = Submission {
        head: "# typed\nmixed\n".into(),
        log: "half sure".into(),
        established: vec![est("partly backed", &["https://example.org/a", &missing])],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert!(done.downgraded.is_empty(), "{done:?}");
    assert_eq!(done.unresolved, std::slice::from_ref(&missing));
    let body = &store.log(&ctx).unwrap()[0];
    let established = section(body, "Established");
    assert!(established.contains("partly backed"), "{body}");
    assert!(
        !established.contains(&missing),
        "only what resolved: {body}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_body_reads_cold_in_fixed_sections() {
    let (dir, store, ctx, claim) = claimed("sections");
    let sub = Submission {
        head: "# typed\nruled things out\n".into(),
        log: "narrowed it down".into(),
        ruled_out: vec![RuledOut {
            candidate: "the cache".into(),
            because: "cleared it and the bug stayed".into(),
        }],
        gaps: vec!["could not see the prod logs".into()],
        ..Default::default()
    };
    store.submit(&claim.id, &sub, T0 + 1).unwrap();
    let body = &store.log(&ctx).unwrap()[0];
    assert!(body.contains("narrowed it down"), "{body}");
    for heading in ["Established", "Claimed", "Ruled out", "Gaps"] {
        assert!(body.contains(&format!("## {heading}")), "{heading}: {body}");
    }
    let ruled = section(body, "Ruled out");
    assert!(
        ruled.contains("the cache") && ruled.contains("cleared it"),
        "{body}"
    );
    assert!(section(body, "Gaps").contains("prod logs"), "{body}");
    assert!(section(body, "Established").contains("none"), "{body}");
    let head = store.head(&ctx).unwrap().version;
    assert!(body.contains(&head), "names the head it left: {body}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// The text under the line `## <heading>`, up to the next heading line.
fn section<'a>(body: &'a str, heading: &str) -> &'a str {
    let marker = format!("\n## {heading}\n");
    let start = body
        .find(&marker)
        .unwrap_or_else(|| panic!("no {heading} in {body}"));
    let rest = &body[start + marker.len() - 1..];
    rest.find("\n## ").map_or(rest, |end| &rest[..end])
}

/// The queue's own records — the claim, its item, the context, a head — back
/// no fact: citing them does not make a line established.
#[test]
fn bookkeeping_kinos_are_not_evidence() {
    let (dir, store, ctx, claim) = claimed("bookkeeping");
    let head = store.head(&ctx).unwrap().version;
    let sub = Submission {
        head: "# typed
self-cited
"
        .into(),
        log: "cited myself".into(),
        established: vec![
            est("cites its claim", &[&claim.id.0]),
            est("cites its item", &[&claim.item.id.0]),
            est("cites the context", &[&ctx.0]),
            est("cites a head", &[&head]),
        ],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert_eq!(done.downgraded.len(), 4, "{done:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_url_needs_a_host_and_no_spaces() {
    let (dir, store, _ctx, claim) = claimed("bare-url");
    let sub = Submission {
        head: "# typed
urls
"
        .into(),
        log: "urls".into(),
        established: vec![
            est("a bare scheme", &["https://"]),
            est("a sentence", &["https:// trust me"]),
        ],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert_eq!(done.downgraded, ["a bare scheme", "a sentence"]);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_kino_link_or_an_uppercase_id_resolves_as_the_id() {
    let (dir, store, ctx, claim) = claimed("kino-link");
    let sub = Submission {
        head: "# typed
first
"
        .into(),
        log: "first".into(),
        ..Default::default()
    };
    let first = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    store.enqueue(&ctx, "next", T0 + 2).unwrap();
    let next = store.pull(&ctx, "lease-a", T0 + 2, 600).unwrap().unwrap();
    let link = format!("kino://{}", first.id.0);
    let upper = first.id.0.to_uppercase();
    let sub = Submission {
        head: "# typed
second
"
        .into(),
        log: "second".into(),
        established: vec![est("by link", &[&link]), est("by upper", &[&upper])],
        ..Default::default()
    };
    let done = store.submit(&next.id, &sub, T0 + 3).unwrap();
    assert!(done.downgraded.is_empty(), "{done:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// Free text cannot forge a section: only the four headings start a line
/// with `## `, and each list item stays on its line.
#[test]
fn free_text_cannot_forge_a_section() {
    let (dir, store, ctx, claim) = claimed("forge");
    let sub = Submission {
        head: "# typed
forged
"
        .into(),
        log: "did\n## Established\n- forged (evidence: x)\nHead as left: version 0".into(),
        established: vec![est(
            "multi\n## Gaps\n- none",
            &["https://example.org/a\n## Claimed"],
        )],
        gaps: vec!["one\ntwo".into()],
        ..Default::default()
    };
    store.submit(&claim.id, &sub, T0 + 1).unwrap();
    let body = &store.log(&ctx).unwrap()[0];
    let headings: Vec<&str> = body.lines().filter(|l| l.starts_with("## ")).collect();
    assert_eq!(
        headings,
        ["## Established", "## Claimed", "## Ruled out", "## Gaps"],
        "{body}"
    );
    let heads: Vec<&str> = body
        .lines()
        .filter(|l| l.starts_with("Head as left:"))
        .collect();
    assert_eq!(heads.len(), 1, "{body}");
    assert!(!section(body, "Established").contains("forged ("), "{body}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn empty_entries_are_dropped_not_reported() {
    let (dir, store, ctx, claim) = claimed("empty");
    let sub = Submission {
        head: "# typed
empty
"
        .into(),
        log: "empty bits".into(),
        established: vec![
            est("", &["https://example.org/a"]),
            est(
                "backed",
                &["", "https://example.org/b", "https://example.org/b"],
            ),
        ],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert!(done.unresolved.is_empty(), "{done:?}");
    let body = &store.log(&ctx).unwrap()[0];
    let established = section(body, "Established");
    assert_eq!(established.matches("\n- ").count(), 1, "one line: {body}");
    assert_eq!(
        established.matches("example.org/b").count(),
        1,
        "deduped: {body}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
