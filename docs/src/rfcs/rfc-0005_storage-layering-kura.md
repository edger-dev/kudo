# RFC-0005: Storage Layering and the Kura Media Archive

- **Status**: Draft
- **Created**: 2026-06-28
- **Project**: Kudo (edger-dev/kudo)
- **Components**: Moco, Kinora, Kura

## Summary

A personal media-archive system — content-addressed photos and videos with rich metadata, collections, generated variations, and multi-place/offline backup — is introduced as a new Kudo component, **Kura** (蔵). Designing it surfaces a deeper architectural decision: the content-addressed *byte-storage substrate* graduates out of Kinora and down into **Moco**, where both Kinora and Kura depend on it. The substrate is a library with pluggable backends; a git-tracked local directory is merely one backend, which preserves Kinora's files-in-git bootstrap while opening the door to central, S3, removable-disk, and pack-file backends.

The result is a clean linear dependency stack — Moco (bytes) ← Kinora (identity/metadata/composition) ← Kura (media domain + viewer) — that dissolves a would-be dependency loop and reuses Kinora's metadata model wholesale for media.

## Motivation

Three threads converge on the same design.

1. **Hub's next feature is a file tool.** The hub MVP (`edger-dev/hub`) already connects containers, a VM, and a macOS environment; its next planned capability was a file access/browse/view/backup tool. That need generalizes into a long-standing desire for a better personal media archive.

2. **Kinora is proving its metadata model.** Kinora is being dogfooded and can already replace beans for basic tracking. Its content-addressed, append-only, provenance-tracked model (RFC-0003) maps almost completely onto media management — what's missing is byte storage at scale, not knowledge modeling.

3. **Kinora's own storage thoughts point off-git.** Independent of media, three desires had emerged for Kinora: stop storing data inside the project's git (the branches aren't wanted), support a central access mode for concurrent multi-writer use without conflict, and add an S3-backed layer with hot/cold tiers. These are *exactly* the storage requirements a media archive demands. The media archive is the forcing function that proves Kinora's storage layer, the same way the RFC documents were the tracer bullet that proved its metadata layer.

### Media use cases

The target experience:

- Import photos/videos after a trip; store content-addressed; **never mutate originals**, but allow future versions (kino-like).
- Maintain searchable/filterable metadata without touching the originals.
- Compose collections — albums, blog posts — either manually curated or dynamically filtered on metadata.
- Attach notes to a single item or to a collection.
- Generate variations — a daily-view-sized image, a thumbnail, a video proxy.
- Keep the whole dataset in multiple places; cold data as never-change pack files.
- Track places that may be offline (removable disks) and run integrity checks.
- Back up selected subsets (e.g. starred) to more places for higher reliability.
- Let a browser/viewer cache different sets per device — a phone holds all thumbnails and starred proxies, never raws — while still making metadata edits (notes) that sync back.

## Design

### Dependency layering

```
Moco        — content-addressed byte storage: pluggable backends,
              tiering, location/replication tracking, integrity, dedup, subscription
  ↑
Kinora      — identity / versioning / metadata / provenance / kinographs
  ↑
Kura (蔵)   — media kinds, variations, EXIF, import, backup policy + Tsui viewer
```

Both Kinora and Kura depend *down* on Moco. There are no upward edges, so the dependency loop that would form if byte storage lived inside Kura (Kura → Kinora → storage-in-Kura) never forms.

This is not a stretch for Moco. RFC-0001 already defines Moco as a content-addressed, immutable, deduplicating data layer with cross-app sharing. The byte substrate *is* Moco's core identity; this RFC moves storage to where it was already specified to live, and the media use cases retroactively answer several of RFC-0001's open questions (size boundaries → whole-file plus chunking; GC strategy → per-collection retention/replication policy; subscription model → multi-device sync).

### Storage as a library with pluggable backends

The load-bearing move: **Moco's storage is a library with pluggable backends, usable embedded (no server) or served (central mode).** "Git-tracked local directory" is just one backend; "central server / S3 / removable disk / pack file" are others.

This single decision pays off across every requirement:

- **Kinora keeps files-in-git.** Its `.kinora/store/` becomes Moco's *local git backend*. The RFC-0003 bootstrap promise — that the data survives as plain files with no running service — stays intact.
- **Central mode is a backend swap**, not a rewrite. Kinora's logic is unchanged; it points at a served backend.
- **Kura gets tiering, offline, and partial replication** from the same backend interface — an S3 cold backend, a removable-disk backend, a pack-file backend — all behind one trait, with the ledger tracking *which backends hold each hash*.
- **No dependency loop**, because the thing both components depend on (the storage library) has no upward edges.

Central mode is then simply "the Moco runtime serving a shared backend over RPC" (RFC-0002), layered on the same library.

### Kinora maps onto media wholesale

Most of the media concept needs no new Kinora concepts:

| Media need | Kinora primitive |
|---|---|
| Import media; never changed; may gain future versions | Kino: identity (birth hash) + content versions |
| Searchable/filterable metadata, originals untouched | Kino metadata (latest-wins per field) |
| Albums, blog posts (manual or dynamic-filtered) | Kinograph (composition; entries by id) |
| Notes on an item or a collection | A kino linked via weak metadata links / kinograph entry notes |
| Provenance (imported from trip X, by whom, when) | Mandatory provenance, already core |
| Starred subset gets extra backup copies | A kinograph + per-collection **replication policy** (generalizes the post-bootstrap per-root GC policy) |

Kinora is already designed to be extended here: RFC-0003 reserves namespaced kinds (`kudo::diagram`, `user::sketch`) with kind-specific renderers, and RFC-0004 anticipates extracting the store + ledger primitives. Media kinds (`kura::photo`, `kura::video`) plus their renderers fit that extension model directly.

**Kura depends on full Kinora, not a `kinora-core` split.** Notes are first-class in the media concept, so the doc/markdown kind and its renderer are wanted regardless. Kura inherits Kinora's whole surface (mdbook rendering, beans sync) but treats those as optional outputs, never load-bearing. A library split is more maintenance cost than it saves here.

### Kura: the media domain

Kura owns what is genuinely media-specific and does not belong in a general substrate:

- **Variation generation** — thumbnails, daily-view proxies, transcodes. Each variation is a derived, content-addressed blob (RFC-0001's derived data) with its *own* replication policy. The derivation *pipeline* is Kura domain logic; the variation *blobs and their link to source* are substrate metadata.
- **Media metadata extractors** — EXIF and equivalents.
- **Import pipelines** — trip/vlog ingest, with dedup-on-import free via content addressing.
- **Backup / offline / partial-replication policy declaration** — Kura declares intent ("thumbnails of all, proxies of starred, raws never on this device"); Moco enforces it.
- **Browser/viewer GUI** — a Moco app plus Tsui renderers (heavy, intentional overlap with RFC-0002).

### The principle that changes

RFC-0003's principle "files in Git are the source of truth" survives for small, mergeable data (the ledger, metadata, kinographs) but **not for media blobs** — they must not enter git and must live across tiers, devices, and offline disks.

Resolution: **decouple the ledger (identity + history + metadata; git-or-central-shaped) from the blob backend (bytes; tiered/replicated/offline-shaped).** The ledger tracks *where* each hash lives; the bytes live wherever a backend puts them. This decoupling is what the "stop storing in git" and "S3 backend" desires were really asking for, and it is the central architectural decision for both Kinora's future and Kura.

### Naming

The component is **Kura** (蔵) — a traditional thick-walled, fireproof, earthquake-resistant storehouse built to keep a family's treasures, art, and records safe across generations. The metaphor maps onto the concept (durable archive of immutable originals, protected by redundancy) and the name sits naturally beside tsui (対). **Canister** (the film can — sealed, immutable, travels between places) is held in reserve as a candidate name for Moco's cold pack-file backend.

## Design Principles

1. **Storage is a library with pluggable backends** — One content-addressed store abstraction, used embedded or served. Local-git, central, S3, removable-disk, and pack-file are all backends behind one trait.

2. **Bytes live below identity** — Moco owns bytes; Kinora owns identity, history, and metadata; Kura owns media domain and presentation. Dependencies point downward only.

3. **Reuse the metadata model, don't fork it** — Media items are kinos, collections are kinographs, notes are linked kinos. Kura adds media-specific kinds and renderers, not a parallel data model.

4. **Decouple the ledger from the blob backend** — Identity/history/metadata may stay git-shaped; bytes are backend-shaped. The ledger records which backends hold each hash.

5. **Domain stays out of the substrate** — Variation pipelines, EXIF, transcoding, and the viewer live in Kura. The substrate stays lean; Kura is its requirements driver.

6. **Kura declares policy; Moco enforces it** — Replication, tiering, and partial-caching intent is expressed against kinographs/metadata queries by Kura and carried out by Moco's backends.

## Sequencing

The work proceeds in user-driven, separately-scoped sessions:

1. **Move store logic Kinora → Moco** — the store/ledger primitives graduate into Moco's storage library; Kinora's `.kinora/store/` becomes the local git backend. (Aligns with RFC-0003's "Post-Bootstrap Evolution," which already anticipates the store/history shape evolving.)
2. **Backend interface** — a dedicated session designing the minimal backend trait and location-tracking model (see Open Questions). This is the load-bearing decision; central mode, S3, offline disks, and Kura all fall out of getting it right.
3. **Stand up Kura** — as the substrate's first real consumer (new repo), proving the storage layer end-to-end.

## Open Questions

- **Backend trait shape** — minimal surface (`put`/`get`/`has`/`list`/integrity-verify) plus capability flags (writable? online? cold/packed?). The keystone everything else hangs off.
- **Where location tracking lives** — does "which backends hold hash X" belong in Moco (storage knows its own replicas) or in Kinora's ledger (metadata knows)? Leaning Moco — replica state is a storage fact, not a knowledge fact — but it is a genuine boundary call.
- **Partial-replication model** — how a device declares "thumbnails of all, proxies of starred, raws never": a policy expressed over kinographs/metadata queries, evaluated by Moco.
- **Ledger in central mode** — does the ledger stay git-backed with git as one mirror, or does central mode introduce a server-owned append log? (Deferred; flagged in this session as not-yet-decided.)
- **Moco's broadened mandate** — Moco moves from "data runtime for the log-viewer tracer bullet" to "universal byte substrate for large media and multi-writer sync." Consistent with RFC-0001's text, but a real scope expansion to make explicit so the storage layer isn't under-scoped.
- **Chunking boundary** — at what size do large media blobs get chunked rather than stored whole (RFC-0001's open size-boundary question, now forced by video).
