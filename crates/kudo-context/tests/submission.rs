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
        established: vec![
            est("the item exists", &[&claim.item.id.0]),
            est("the docs say so", &["https://example.org/docs#x"]),
        ],
        ..Default::default()
    };
    let done = store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert!(done.downgraded.is_empty(), "{done:?}");
    assert!(done.unresolved.is_empty(), "{done:?}");

    let body = &store.log(&ctx).unwrap()[0];
    let established = section(body, "Established");
    assert!(established.contains("the item exists"), "{body}");
    assert!(established.contains(&claim.item.id.0), "{body}");
    assert!(established.contains("https://example.org/docs#x"), "{body}");
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
        established: vec![est("partly backed", &[&ctx.0, &missing])],
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

/// The text under `## <heading>`, up to the next heading.
fn section<'a>(body: &'a str, heading: &str) -> &'a str {
    let start = body
        .find(&format!("## {heading}\n"))
        .unwrap_or_else(|| panic!("no {heading} in {body}"));
    let rest = &body[start + heading.len() + 4..];
    rest.find("\n## ").map_or(rest, |end| &rest[..end])
}
