//! Completed sessions leave raw and distilled records; distilled spans are evidence.
//!
//! implements: 0139122baa08b2f39720a8b6fb15e848c96ca7fb8ccdd6056ae9e2287e0397ea

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use kudo_context::{ContextStore, Established, Submission};

const T0: i64 = 1_792_500_000;

#[test]
fn raw_and_distilled_records_are_linked_and_spans_resolve() {
    let dir = std::env::temp_dir().join(format!("kudo-context-transcript-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let store = ContextStore::init(&dir).unwrap();
    let ctx = store.create_context("trace", "# trace\n", T0).unwrap();
    let raw = br#"{"type":"user","message":{"role":"user","content":"hello"}}
{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"worked"}]}}
"#;
    let distilled = concat!(
        "{\"line\":1,\"speaker\":\"user\",\"kind\":\"user\",\"text\":\"hello\",\"ts\":\"t0\"}\n",
        "{\"line\":2,\"speaker\":\"assistant\",\"kind\":\"assistant\",\"text\":\"worked\",\"ts\":\"t1\"}\n"
    );
    let ids = store
        .file_transcript(&ctx, "lease-a", raw, distilled, T0)
        .unwrap();

    assert_ne!(ids.raw, ids.distilled);
    assert_eq!(store.transcript(&ids.raw).unwrap().as_bytes(), raw);
    assert_eq!(store.transcript(&ids.distilled).unwrap(), distilled);
    assert_eq!(
        store.transcripts(&ctx, "lease-a").unwrap(),
        vec![ids.clone()]
    );

    store.enqueue(&ctx, "cite the session", T0 + 1).unwrap();
    let claim = store.pull(&ctx, "lease-b", T0 + 1, 600).unwrap().unwrap();
    let done = store
        .submit(
            &claim.id,
            &Submission {
                head: "cited\n".into(),
                log: "cited".into(),
                established: vec![Established {
                    claim: "the worker said it worked".into(),
                    evidence: vec![format!("transcript:{}:2-2", ids.distilled)],
                }],
                ..Default::default()
            },
            T0 + 2,
        )
        .unwrap();
    assert!(done.downgraded.is_empty(), "{done:?}");
    assert!(done.unresolved.is_empty(), "{done:?}");

    let _ = std::fs::remove_dir_all(&dir);
}
