# RFC-0003: Kinora Bootstrap

- **Status**: Draft
- **Created**: 2026-04-10
- **Project**: Kudo (edger-dev/kudo)
- **Components**: Kinora
- **Repository**: edger-dev/kudo.kinora

## Summary

Kinora can be bootstrapped as files in a Git repository before any application code exists. Since Kinora's design positions files in Git as the source of truth — with all other representations (indices, rendered views, search) as derived tooling — there is nothing preventing the system from operating as a manually-managed file structure with minimal CLI tooling. This RFC describes the bootstrap approach: a content-addressed file store, an append-only ledger, and a small CLI that enforces the invariants.

## Motivation

The Kudo project is generating design knowledge through exploratory discussions — conversations about data models, architectural boundaries, filtering strategies, and system principles. This knowledge needs to be captured in a way that:

- Preserves the append-only, versioned nature of evolving ideas (an early design note is not "wrong" when thinking evolves — it is a historical version)
- Supports composition (assembling related kinos into coherent topic views without folder hierarchies)
- Tests Kinora's own design principles before any Kinora application code is written
- Remains useful as plain files in Git even if tooling changes completely

The bootstrap approach treats the documentation process itself as a tracer bullet for Kinora's knowledge management design.

## Design

### Content Store

All content is stored by hash. When a kino (a semantic knowledge fragment) is created, its content is hashed, and the file is stored at a path derived from that hash.

```
store/
  ab/
    ab3f7c...  (content blob)
  d1/
    d1e92a...  (content blob)
```

The path structure uses the first two characters of the hash as a directory prefix (following Git's convention) to avoid filesystem performance issues with large flat directories.

Content files are pure content — no metadata, no frontmatter, no headers added by the system. The content you put in is exactly the content stored. This means the hash is deterministic and verifiable: anyone can hash the file and confirm it matches its path.

A human never directly creates or names a file in the content store. Content enters only through a tool that hashes and places it.

### Ledger

The ledger is an append-only file (or set of files) that records every meaningful event:

- A new kino version was created (name, hash, author, provenance, timestamp)
- A kinograph was created or updated (name, list of kino references, author, timestamp)

The ledger is how you answer questions like "what is the latest version of the filtering-boundary kino?" or "who authored this content and where did it come from?" The content store alone cannot answer these questions — it only knows hashes and content.

The ledger is append-only: entries are never modified or deleted. To supersede a kino version, you append a new entry pointing to new content. The old entry and old content remain.

### Kinographs

A kinograph is a composition — a file that references specific content hashes to assemble a coherent view of a topic. A kinograph about "Moco Data Model" might reference the content hashes of kinos about content addressing, the reference layer, deduplication, and derived data.

Kinographs are themselves stored in the content store (they are content, after all) and tracked in the ledger. Updating a kinograph means creating a new version with updated references.

### Provenance

Every ledger entry includes provenance: who created this content and where it came from. For the bootstrap phase, provenance is simple text — "derived from Claude conversation on 2026-04-10" or "authored by YJ after prototype experiment." This tests Kinora's mandatory provenance principle with zero tooling complexity.

### Rendering

For the bootstrap phase, mdbook serves as the first "mosaic renderer." A generator reads the ledger and kinographs, resolves hashes to content, and produces:

- A `SUMMARY.md` file assembling kinographs into a readable structure
- Rendered markdown pages with source path markers showing the original content-addressed file path

The markers create a breadcrumb trail: anyone reading the rendered site can trace back to the exact content-addressed blob. When a proper Kinora viewer replaces mdbook, the underlying data is untouched — only the rendering layer changes.

### Beans Sync

Actionable items identified in kinographs can be exported to beans task specs. A generator reads kinographs, identifies items tagged or structured as tasks, and produces a spec file that beans can consume. The kinograph remains the source of truth; the beans spec is a derived artifact, regenerated on demand.

This follows the same pattern as mdbook rendering: Kinora data is the source, everything else is a view.

## Minimal CLI

The bootstrap requires a small CLI that enforces content addressing and ledger invariants. Three commands are sufficient to start:

### `store`

Takes content (from stdin or a file path), hashes it, writes it to the content store at the hash-derived path, and appends a ledger entry with metadata (kino name, author, provenance, timestamp).

If the content already exists in the store (same hash), the store is not modified, but a new ledger entry is still appended (because a new version of a named kino might point to previously-existing content).

### `resolve`

Given a kino name, reads the ledger to find the latest entry for that name and returns the content from the store. Can also resolve a specific version (by timestamp or version number) rather than latest.

### `render`

Walks kinographs, resolves all referenced hashes, and produces mdbook-compatible output — SUMMARY.md and assembled markdown files with source markers.

## Bootstrap Sequence

1. Create the `kudo.kinora` repository with the content store and ledger structure
2. Implement the minimal CLI (store, resolve, render)
3. Migrate the RFC documents from this conversation into the content store as the first kinos
4. Create kinographs that compose these kinos into topic views
5. Set up mdbook rendering as the first derived view
6. Use the system going forward for all Kudo design documentation

The RFCs themselves (including this one) become the first content managed by the system they describe.

## Design Principles

1. **Files in Git are the source of truth** — All other representations (rendered sites, search indices, task specs) are derived and regeneratable. If the tooling breaks, the data survives as plain files in Git.

2. **Content addressing is enforced, not conventional** — The CLI enforces that content enters the store by hash. This is not a convention that humans maintain — it is a mechanical invariant.

3. **Append-only by structure** — The ledger only grows. Kino versions are never overwritten. This property is enforced by the data layout, not by discipline.

4. **Provenance is mandatory** — Every ledger entry records who created the content and where it came from. There is no anonymous content.

5. **Rendering is always derived** — mdbook output, beans specs, and any future views are generated from the source data. They are never edited directly.

## Open Questions

- Hash algorithm choice — needs to align with RFC-0001's Moco data model decision
- Ledger format — structured text (one entry per line, tab-separated fields), JSON lines, or something else?
- Kinograph format — how to express references to content hashes in a way that is both human-readable and machine-parseable
- Conflict handling — when two people append to the ledger concurrently in different Git branches, how does merge work? (Append-only makes this simpler but not trivial)
- Should the CLI be a standalone tool or part of a broader Kinora crate from the start?
- Link format — how kinos reference other kinos (by name? by hash? by both?) and how bidirectional links are tracked
