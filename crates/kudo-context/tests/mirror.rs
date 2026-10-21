//! Two machines, one mirror: stores that share a mirror act as one store —
//! enqueues, claims and heads cross over; writes made offline push later.
//!
//! implements: fc473f56b1b8bdbae4a31e54e7281c4aeed3c08bd03804b256ca37e1937fe66f@fc473f56b1b8bdbae4a31e54e7281c4aeed3c08bd03804b256ca37e1937fe66f

#![allow(clippy::unwrap_used, clippy::expect_used, reason = "tests")]

use std::path::{Path, PathBuf};

use kudo_context::{ContextStore, Error, Mirror, Submission};

const T0: i64 = 1_792_500_000;

fn scratch(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("kudo-context-mirror-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A machine's store, mirrored to the bucket at `bucket`.
fn machine(dir: &Path, bucket: &Path) -> ContextStore {
    let mirror = Mirror::from_url(&format!("file://{}", bucket.display())).unwrap();
    // No spacing: each read here stands for a separate process.
    ContextStore::init(dir)
        .unwrap()
        .with_mirror(mirror)
        .with_sync_spacing(std::time::Duration::ZERO)
        .with_offline_backoff(std::time::Duration::ZERO)
}

fn sub(head: &str) -> Submission {
    Submission {
        head: head.into(),
        log: "done".into(),
        ..Default::default()
    }
}

#[test]
fn an_item_enqueued_on_one_machine_is_ready_on_the_other() {
    let root = scratch("enqueue");
    let bucket = root.join("bucket");
    let pc = machine(&root.join("pc"), &bucket);
    let edger = machine(&root.join("edger"), &bucket);

    let ctx = pc.create_context("shared", "# shared\n", T0).unwrap();
    pc.enqueue(&ctx, "from pc", T0).unwrap();

    let seen: Vec<_> = edger
        .contexts()
        .unwrap()
        .into_iter()
        .map(|c| c.id)
        .collect();
    assert_eq!(seen, std::slice::from_ref(&ctx));
    assert_eq!(edger.ready_count(&ctx, T0).unwrap(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_claim_on_one_machine_is_seen_before_the_other_claims() {
    let root = scratch("claim");
    let bucket = root.join("bucket");
    let pc = machine(&root.join("pc"), &bucket);
    let edger = machine(&root.join("edger"), &bucket);
    let ctx = pc.create_context("shared", "# shared\n", T0).unwrap();
    pc.enqueue(&ctx, "only one", T0).unwrap();

    assert!(pc.pull(&ctx, "lease-pc", T0, 600).unwrap().is_some());
    assert_eq!(edger.pull(&ctx, "lease-edger", T0, 600).unwrap(), None);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_submit_on_a_head_the_other_machine_moved_is_refused() {
    let root = scratch("moved");
    let bucket = root.join("bucket");
    let pc = machine(&root.join("pc"), &bucket);
    let edger = machine(&root.join("edger"), &bucket);
    let ctx = pc.create_context("shared", "# shared\n", T0).unwrap();
    pc.enqueue(&ctx, "one", T0).unwrap();
    pc.enqueue(&ctx, "two", T0).unwrap();

    let on_pc = pc.pull(&ctx, "lease-pc", T0, 600).unwrap().unwrap();
    let on_edger = edger.pull(&ctx, "lease-edger", T0, 600).unwrap().unwrap();
    assert_ne!(on_pc.item.id, on_edger.item.id);

    pc.submit(&on_pc.id, &sub("# shared\npc was here\n"), T0 + 1)
        .unwrap();
    assert!(matches!(
        edger.submit(&on_edger.id, &sub("# shared\nedger\n"), T0 + 2),
        Err(Error::HeadMoved { .. })
    ));
    let _ = std::fs::remove_dir_all(&root);
}

/// A machine that cannot reach the mirror keeps working on its own store;
/// its writes push on the first operation that reaches the mirror again.
#[test]
fn writes_made_offline_push_when_the_mirror_is_back() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("offline");
    let bucket = root.join("bucket");
    let pc = machine(&root.join("pc"), &bucket);
    let edger = machine(&root.join("edger"), &bucket);
    let ctx = pc.create_context("shared", "# shared\n", T0).unwrap();
    assert_eq!(edger.contexts().unwrap().len(), 1);

    std::fs::set_permissions(&bucket, std::fs::Permissions::from_mode(0o000)).unwrap();
    let offline = pc.enqueue(&ctx, "written offline", T0 + 1);
    std::fs::set_permissions(&bucket, std::fs::Permissions::from_mode(0o755)).unwrap();
    offline.unwrap();
    assert_eq!(pc.ready_count(&ctx, T0 + 2).unwrap(), 1, "kept locally");

    pc.head(&ctx).unwrap(); // any operation reaching the mirror again
    assert_eq!(edger.ready_count(&ctx, T0 + 3).unwrap(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

/// A machine that can read the mirror but not write to it still pulls:
/// it must see the other machine's claims even when its own push fails.
#[test]
fn a_failed_push_still_pulls() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("push-fails");
    let bucket = root.join("bucket");
    let pc = machine(&root.join("pc"), &bucket);
    let edger = machine(&root.join("edger"), &bucket);
    let ctx = pc.create_context("shared", "# shared\n", T0).unwrap();
    assert_eq!(edger.contexts().unwrap().len(), 1);

    // A blob edger cannot read: every push of edger's fails.
    let shard = root.join("edger/.kinora/store/ab");
    std::fs::create_dir_all(&shard).unwrap();
    let blob = shard.join(format!("{}.md", "ab".repeat(32)));
    std::fs::write(&blob, "unreadable").unwrap();
    std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o000)).unwrap();

    pc.enqueue(&ctx, "from pc", T0 + 1).unwrap();
    let ready = edger.ready_count(&ctx, T0 + 2);
    std::fs::set_permissions(&blob, std::fs::Permissions::from_mode(0o644)).unwrap();
    assert_eq!(ready.unwrap(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

/// After the mirror failed, a process does not retry it on every call: it
/// works locally until the backoff passes.
#[test]
fn an_unreachable_mirror_is_not_retried_until_the_backoff_passes() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("backoff");
    let bucket = root.join("bucket");
    let pc = machine(&root.join("pc"), &bucket)
        .with_offline_backoff(std::time::Duration::from_secs(600));
    let edger = machine(&root.join("edger"), &bucket);
    let ctx = pc.create_context("shared", "# shared\n", T0).unwrap();

    std::fs::set_permissions(&bucket, std::fs::Permissions::from_mode(0o000)).unwrap();
    let offline = pc.enqueue(&ctx, "offline", T0 + 1);
    std::fs::set_permissions(&bucket, std::fs::Permissions::from_mode(0o755)).unwrap();
    offline.unwrap();

    pc.head(&ctx).unwrap(); // within the backoff: no sync
    assert_eq!(edger.ready_count(&ctx, T0 + 2).unwrap(), 0);
    let _ = std::fs::remove_dir_all(&root);
}

/// Two machines claimed one item while apart; once both claims are in the
/// mirror, the store itself refuses the losing lease's submit and release.
#[test]
fn the_losing_lease_of_a_race_cannot_submit_or_release() {
    use std::os::unix::fs::PermissionsExt;
    let root = scratch("race");
    let bucket = root.join("bucket");
    let pc = machine(&root.join("pc"), &bucket);
    let edger = machine(&root.join("edger"), &bucket);
    let ctx = pc.create_context("shared", "# shared\n", T0).unwrap();
    pc.enqueue(&ctx, "contended", T0).unwrap();
    assert_eq!(edger.ready_count(&ctx, T0).unwrap(), 1);

    std::fs::set_permissions(&bucket, std::fs::Permissions::from_mode(0o000)).unwrap();
    let on_pc = pc.pull(&ctx, "lease-z", T0 + 1, 600);
    std::fs::set_permissions(&bucket, std::fs::Permissions::from_mode(0o755)).unwrap();
    let on_pc = on_pc.unwrap().unwrap();
    let on_edger = edger.pull(&ctx, "lease-a", T0 + 2, 600).unwrap().unwrap();
    assert_eq!(on_pc.id, on_edger.id, "one attempt, two births");
    pc.head(&ctx).unwrap(); // pc's claim reaches the mirror

    assert!(matches!(
        pc.submit_as("lease-z", &on_pc.id, &sub("# shared\nz\n"), T0 + 3),
        Err(Error::ClaimNotHeld { .. })
    ));
    assert!(matches!(
        pc.release_as("lease-z", &on_pc.id, T0 + 3),
        Err(Error::ClaimNotHeld { .. })
    ));
    edger
        .submit_as("lease-a", &on_edger.id, &sub("# shared\na\n"), T0 + 4)
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
}

/// A mirror that refuses the credentials is a misconfiguration, not an
/// outage: it is an error, never a silent fall back to local work.
#[test]
fn a_mirror_refusing_the_credentials_is_an_error() {
    let root = scratch("unauth");
    let mirror = Mirror::new(std::sync::Arc::new(Refusing), "ctx");
    let store = ContextStore::init(&root.join("pc"))
        .unwrap()
        .with_mirror(mirror)
        .with_sync_spacing(std::time::Duration::ZERO);
    assert!(
        matches!(store.contexts(), Err(Error::Mirror(_))),
        "{:?}",
        store.contexts()
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// An object store that refuses every request as unauthenticated.
#[derive(Debug)]
struct Refusing;

impl std::fmt::Display for Refusing {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Refusing")
    }
}

fn refused() -> object_store::Error {
    object_store::Error::Unauthenticated {
        path: "ctx".into(),
        source: "bad key".into(),
    }
}

#[async_trait::async_trait]
impl object_store::ObjectStore for Refusing {
    async fn put_opts(
        &self,
        _: &object_store::path::Path,
        _: object_store::PutPayload,
        _: object_store::PutOptions,
    ) -> object_store::Result<object_store::PutResult> {
        Err(refused())
    }
    async fn put_multipart_opts(
        &self,
        _: &object_store::path::Path,
        _: object_store::PutMultipartOptions,
    ) -> object_store::Result<Box<dyn object_store::MultipartUpload>> {
        Err(refused())
    }
    async fn get_opts(
        &self,
        _: &object_store::path::Path,
        _: object_store::GetOptions,
    ) -> object_store::Result<object_store::GetResult> {
        Err(refused())
    }
    fn delete_stream(
        &self,
        _: futures::stream::BoxStream<'static, object_store::Result<object_store::path::Path>>,
    ) -> futures::stream::BoxStream<'static, object_store::Result<object_store::path::Path>> {
        Box::pin(futures::stream::once(async { Err(refused()) }))
    }
    fn list(
        &self,
        _: Option<&object_store::path::Path>,
    ) -> futures::stream::BoxStream<'static, object_store::Result<object_store::ObjectMeta>> {
        Box::pin(futures::stream::once(async { Err(refused()) }))
    }
    async fn list_with_delimiter(
        &self,
        _: Option<&object_store::path::Path>,
    ) -> object_store::Result<object_store::ListResult> {
        Err(refused())
    }
    async fn copy_opts(
        &self,
        _: &object_store::path::Path,
        _: &object_store::path::Path,
        _: object_store::CopyOptions,
    ) -> object_store::Result<()> {
        Err(refused())
    }
}
