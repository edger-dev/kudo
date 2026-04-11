# RFC-0001: Moco Data Model

- **Status**: Draft
- **Created**: 2026-04-10
- **Project**: Kudo (edger-dev/kudo)
- **Components**: Moco

## Summary

Moco manages data as immutable, content-addressed objects. All data entering Moco — whether application content, runtime information, or derived artifacts — is stored once, referenced by hash, and never mutated. This design draws from Git's object model and functional programming principles: you never change data, you only add new data and update references.

## Motivation

Traditional application runtimes treat data as mutable state. This creates cascading complexity: cache invalidation, concurrent access conflicts, synchronization between views, undo/redo machinery. Each application reinvents solutions to these problems, usually incompletely.

By making immutability a foundational property of the data layer, Moco eliminates entire categories of problems at the architectural level rather than leaving them to individual applications.

## Design

### Two-Layer Data Model

Moco's data model consists of two distinct layers:

**Content Layer** — pure content-addressed blobs. Any piece of data (a JSON object, a markdown document, a configuration fragment) is stored by the hash of its content. Identical content always resolves to the same address, regardless of when or where it was introduced. This is the storage layer: content in, hash out, content never duplicated.

**Reference Layer** — ordered sequences, indices, and pointers that give structure to content. A log file is not stored as a single blob; it becomes an ordered list of content hashes. A document with sections becomes a sequence of references. References encode relationships and ordering without duplicating the underlying content.

This separation means that the same content blob can appear in multiple reference structures without any additional storage cost, and that structural reorganization (reordering, filtering, grouping) operates only on lightweight references, never touching the content itself.

### Content Addressing

Every blob stored in Moco is addressed by a deterministic hash of its content. The specific hash algorithm is an implementation detail, but the properties are fixed:

- Same content always produces the same hash
- Different content always produces different hashes (within practical collision bounds)
- The hash is the sole identifier — there are no separate "names" or "IDs" at the content layer

### Immutability

Once a blob is stored, it cannot be modified. There is no update operation at the content layer. To represent a change, you store new content (which gets a new hash) and update references to point to the new hash. The old content remains addressable by its original hash.

This applies to all data Moco manages, including runtime information. State changes are represented as new immutable snapshots, not mutations of existing objects.

### Deduplication

Content addressing provides automatic deduplication. If the same error message appears 10,000 times in a log, Moco stores the content once. The reference layer (the ordered list of event hashes) contains 10,000 entries, but they all point to the same blob.

This property extends to cross-application deduplication. If two different Moco apps ingest the same content independently, they produce the same hash and share the same stored blob without any coordination.

### Derived Data

When a transformation is deterministic and reusable, its output can itself be stored as content-addressed Moco data. For example, parsing a JSONL file into individual structured events produces derived blobs — each event becomes its own content-addressed object, and the parsed result becomes a new reference list.

The principle: **if a transformation is deterministic and its output would be useful to other consumers, the result belongs in Moco as derived immutable data. If the transformation is presentational and ephemeral, it stays outside Moco** (e.g. in Tsui's view model layer).

## Moco as Essential Blocks

Rather than defining Moco as a monolithic runtime, it is composed of a few essential capabilities:

- **Data management** — the content-addressed store and reference layer described above
- **Plugin management** — registration and lifecycle of Moco apps (thin programs that produce or consume Moco data)
- **RPC endpoint** — the interface through which apps and external systems (including Tsui) query, subscribe to, and ingest data

Each Moco app is a thin, focused program. A log file watcher app reads a file, splits content into entries, and stores them as content-addressed blobs with an ordered reference list. A viewer app reads those blobs and reference lists through Moco's query interface. The apps do not communicate directly — they communicate through data in Moco.

## Design Principles

1. **Immutability as default** — All data in Moco is immutable once stored. Mutability is never introduced as a convenience; instead, new versions are new data.

2. **Content addressing as identity** — Data is identified by what it contains, not by where it lives or what created it. This eliminates naming conflicts, enables automatic deduplication, and makes caching trivially correct.

3. **Separation of content and structure** — What something *is* (content) and how it *relates to other things* (references) are stored independently. This allows the same content to participate in multiple structures without duplication.

4. **Thin apps, rich data** — Moco apps are deliberately minimal. Complexity lives in the data model and the relationships between data, not in application logic. Apps are producers and consumers of immutable data, not managers of mutable state.

5. **Aggressive cacheability** — Because content never changes and is addressed by hash, any layer of the system can cache content indefinitely without invalidation logic. This is not an optimization — it is a structural property of the design.

## Open Questions

- Specific hash algorithm selection (SHA-256, BLAKE3, etc.) — tradeoffs between speed, collision resistance, and hash length
- Garbage collection strategy for content blobs that are no longer referenced
- Reference layer format — how ordered lists and structural references are themselves stored (are they also content-addressed?)
- Subscription/notification model — how consumers learn about new data without polling
- Size boundaries — at what scale does content-addressing individual lines vs. chunks vs. whole files make sense?
