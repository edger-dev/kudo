//! A context's repos: a declaration of its own, by name and node, that a
//! worker starts in by name — and whose ledgers back a submission's evidence.
//!
//! implements: b53b7abd832f2cd0e58e5084f5733af5d56f762c52cd6fd404913911920a6de9@b53b7abd832f2cd0e58e5084f5733af5d56f762c52cd6fd404913911920a6de9

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::{Path, PathBuf};

use kudo_context::{ContextId, ContextStore, Error, Established, Repo, Submission};

const T0: i64 = 1_792_500_000;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("kudo-context-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn store_with_context(name: &str) -> (PathBuf, ContextStore, ContextId) {
    let dir = scratch(name);
    let store = ContextStore::init(&dir.join("store")).unwrap();
    let ctx = store
        .create_context("spans repos", "# spans\n", T0)
        .unwrap();
    (dir, store, ctx)
}

fn repo(name: &str, node: &str, path: &str) -> Repo {
    Repo {
        name: name.into(),
        node: node.into(),
        path: PathBuf::from(path),
    }
}

#[test]
fn repos_are_declared_by_name_and_node_and_a_redeclare_moves_the_path() {
    let (dir, store, ctx) = store_with_context("repos-declare");
    assert!(store.repos(&ctx).unwrap().is_empty());

    store
        .declare_repo(&ctx, &repo("kudo", "yolo", "/w/kudo"), T0)
        .unwrap();
    store
        .declare_repo(&ctx, &repo("hub", "yolo", "/w/hub"), T0 + 1)
        .unwrap();
    store
        .declare_repo(&ctx, &repo("kudo", "pc", "/src/kudo"), T0 + 2)
        .unwrap();
    store
        .declare_repo(&ctx, &repo("kudo", "yolo", "/w2/kudo"), T0 + 3)
        .unwrap();

    let repos = store.repos(&ctx).unwrap();
    assert_eq!(
        repos,
        [
            repo("hub", "yolo", "/w/hub"),
            repo("kudo", "pc", "/src/kudo"),
            repo("kudo", "yolo", "/w2/kudo"),
        ]
    );

    let other = store.create_context("other", "# other\n", T0).unwrap();
    assert!(store.repos(&other).unwrap().is_empty(), "per context");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_repo_is_found_by_name_on_a_node() {
    let (dir, store, ctx) = store_with_context("repos-on");
    store
        .declare_repo(&ctx, &repo("kudo", "yolo", "/w/kudo"), T0)
        .unwrap();

    assert_eq!(
        store.repo_on(&ctx, "kudo", "yolo").unwrap(),
        repo("kudo", "yolo", "/w/kudo")
    );
    assert!(matches!(
        store.repo_on(&ctx, "kudo", "pc"),
        Err(Error::UnknownRepo { .. })
    ));
    assert!(matches!(
        store.repo_on(&ctx, "hub", "yolo"),
        Err(Error::UnknownRepo { .. })
    ));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_bad_declaration_is_refused() {
    let (dir, store, ctx) = store_with_context("repos-bad");
    for bad in [
        repo("two words", "yolo", "/w"),
        repo("kudo", "", "/w"),
        repo("kudo", "yo@lo", "/w"),
        repo("kudo", "yolo", "relative/path"),
    ] {
        assert!(
            matches!(
                store.declare_repo(&ctx, &bad, T0),
                Err(Error::InvalidRepo(_))
            ),
            "{bad:?}"
        );
    }
    assert!(store.repos(&ctx).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_submit_never_touches_the_repos() {
    let (dir, store, ctx) = store_with_context("repos-submit");
    store
        .declare_repo(&ctx, &repo("kudo", "yolo", "/w/kudo"), T0)
        .unwrap();
    store.enqueue(&ctx, "work", T0).unwrap();
    let claim = store.pull(&ctx, "lease-a", T0, 600).unwrap().unwrap();
    let sub = Submission {
        head: "# spans\nworked\n".into(),
        log: "worked".into(),
        ..Default::default()
    };
    store.submit(&claim.id, &sub, T0 + 1).unwrap();
    assert_eq!(
        store.repos(&ctx).unwrap(),
        [repo("kudo", "yolo", "/w/kudo")]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// A kino in the ledger of a declared repo is evidence; its bytes are copied
/// into the context store, so it still resolves when the repo is gone.
// implements: 4e67251bc978f34e7d6ce4278eb1503f9e803e51f0057d4d5954c701830f22b9@1b7ca6a98f08e1312c89a21ba22a046c2f9677965e58dd2fde0cae2756307a74
#[test]
fn a_kino_in_a_declared_repo_ledger_is_evidence_and_is_cached() {
    let (dir, store, ctx) = store_with_context("repos-cite");
    let repo_dir = dir.join("project");
    let spec = repo_kino(&repo_dir, "# A spec in the project\n");
    store
        .declare_repo(
            &ctx,
            &Repo {
                name: "project".into(),
                node: "yolo".into(),
                path: repo_dir.clone(),
            },
            T0,
        )
        .unwrap();

    let cite = |ask: &str, at: i64| {
        store.enqueue(&ctx, ask, at).unwrap();
        let claim = store.pull(&ctx, "lease-a", at, 600).unwrap().unwrap();
        let sub = Submission {
            head: format!("# spans\n{ask}\n"),
            log: ask.into(),
            established: vec![Established {
                claim: "the project decided it".into(),
                evidence: vec![spec.clone()],
            }],
            ..Default::default()
        };
        store.submit(&claim.id, &sub, at + 1).unwrap()
    };

    let first = cite("with the repo", T0 + 1);
    assert!(first.downgraded.is_empty(), "{first:?}");

    std::fs::remove_dir_all(&repo_dir).unwrap();
    let second = cite("without the repo", T0 + 10);
    assert!(second.downgraded.is_empty(), "cached: {second:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

/// A project ledger with one kino in it; its id.
fn repo_kino(repo: &Path, content: &str) -> String {
    std::fs::create_dir_all(repo).unwrap();
    kinora::init::init(repo, "https://example.org/project").unwrap();
    let stored = kinora::kino::store_kino(
        &kinora::paths::kinora_root(repo),
        kinora::kino::StoreKinoParams {
            kind: "markdown".into(),
            content: content.as_bytes().to_vec(),
            author: "test".into(),
            provenance: "test".into(),
            ts: "2026-10-21T00:00:00Z".into(),
            metadata: [("name".to_owned(), "a-spec".to_owned())].into(),
            id: None,
            parents: Vec::new(),
        },
    )
    .unwrap();
    stored.event.id
}
