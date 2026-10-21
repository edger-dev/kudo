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
