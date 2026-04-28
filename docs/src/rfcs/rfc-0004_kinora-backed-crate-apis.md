# RFC-0004: Kinora-Backed Crate APIs

- **Status**: Draft
- **Created**: 2026-04-28
- **Project**: Kudo (edger-dev/kudo)
- **Components**: Stencil, Kinora

## Summary

Crates published from kudo (anything under `crates/`) define their public API as a [kinora](https://github.com/edger-dev/kinora) kinograph. A new preprocessor — **stencil** — walks the kinograph and renders the API surface as read-only sections inside source files, with editable bodies for implementation. The mechanism mixes preprocessor-managed and human/agent-managed code in the same file, language-agnostically, following silp's authoring model.

The result is a structural separation between API contract (kinora-managed, content-addressed, append-only) and implementation (in-source, freely edited within editable regions). The boundary is convention-enforced at bootstrap and evolves toward stronger structural enforcement as patterns settle.

## Motivation

A crate's API is its single most important contract — the surface every downstream consumer depends on, the interface every implementation must honor. For the crates kudo intends to publish, API drift is the leading source of long-term tech debt: each unintended change propagates as a semver bump, broken consumers, and accumulated workarounds.

Coding agents amplify this risk. Without a structural boundary, an agent implementing a function can quietly adjust its signature to make the implementation easier — silently changing the contract. Convention alone doesn't hold: the boundary needs to live in the code structure, visible to both humans and agents.

Three observations motivate the design:

1. **Kinora is built for this kind of contract.** Its content-addressed, append-only model already provides immutable versioning, provenance, and composition (RFC-0003). API specs are exactly the kind of artifact that benefits from these properties.

2. **silp has already proven the mixing model.** Years of usage of [silp](https://github.com/yjpark/silp) showed that interleaving preprocessor-managed read-only sections with hand-edited regions inside a single source file works smoothly across languages and is friendly to incremental adoption.

3. **Prototypes earn the right to become crates.** Reusable APIs benefit from being designed only after multiple prototypes have shown what's actually needed. Prototypes are exploratory and short-lived; crates are deliberate and published. Treating the spec workflow as the boundary between these two modes is what gives the workflow its weight.

The agentic workflow that *uses* this mechanism — design sessions, coding tasks, the API change lifecycle — is out of scope here and will be specified in a future kudo workflow RFC. This RFC defines the substrate that workflow will sit on.

## Design

### Scope of Application

The spec-driven workflow applies by **convention**, based on workspace location:

| Location | Treatment | Notes |
|---|---|---|
| `crates/` | Spec-driven; kinora-backed; stencil-managed read-only sections | Crates intended for publishing live here |
| Anywhere else with a `Cargo.toml` | Free-form | Prototypes, experiments, internal tools — agents have full freedom |
| Opt-in (e.g. a web app) | Spec-driven for chosen surfaces (e.g. GUI contracts) | Same machinery, applied selectively |

Prototypes do not have a designated folder. Any Cargo package outside `crates/` is a prototype unless declared otherwise. The rule stays simple: **if it's under `crates/`, it has an API kinograph; if it's not, it doesn't.**

### Spec Representation: API as Kinograph

A crate's API is expressed as a kinograph — a kinora composition that references the kinos describing the crate's public surface. Two kinds of kinos compose into an API:

- **Atomic kinos** capture a single shared, stable element of the public surface — a foundational error type, a core ID newtype, a trait that multiple crates implement. These are the items where reuse via composition is real: the same kino can be referenced from several crates' API kinographs.

- **Logical-unit kinos** capture a cohesive crate-specific concept — a struct together with its constructors and invariants, or a trait together with its associated types and contracts. The exact shape of "logical unit" is not over-specified at bootstrap; it will be discovered through prototyping.

A crate's API kinograph composes both. The expectation is that *most* of an early crate's API is logical-unit kinos; atomic kinos emerge as patterns recur across crates and earn extraction.

### Spec Content

Each spec kino captures:

- **Signatures** — function/method/type signatures, trait definitions, visibility, generics, error types
- **Behavioral contracts** — preconditions, invariants, error semantics, examples; written as prose at bootstrap

At bootstrap, behavioral contracts are prose only. Tests are entirely agent-authored against the prose contracts; coverage is a discipline goal, not a structural property. The evolution path (see Post-Bootstrap Evolution) introduces hybrid mechanisms — doctests for simple assertions, named test scaffolds for complex cases, prose-only as fallback.

### Stencil: the Preprocessor

Stencil is a new tool, silp-inspired in its mixing model, kinora-native in its inputs, and language-agnostic in its design. Its job:

1. **Read** an API kinograph from kinora
2. **Resolve** the kinograph's entries (atomic and logical-unit kinos) to their content
3. **Render** the resolved spec as read-only sections in target source files, using language-appropriate comment markers
4. **Preserve** the editable regions between markers untouched

Stencil is the sole author of read-only sections. A re-run produces the same sections (modulo kinograph version), regardless of what the editable regions contain.

Stencil is itself spec-driven from day one — its own public API is defined as a kinograph and rendered into its source files via a bootstrap step. This dogfoods the system before any other crate adopts it.

### Mixed Authorship in Source Files

A spec-driven Rust file mixes preprocessor-managed and human/agent-managed regions, marked by language-appropriate comment conventions (silp-style). Read-only sections contain signatures, type definitions, and doc-comments with prose contracts. Editable regions contain function bodies, private helpers, tests, and free-form documentation.

This single-file mixing is the essential ergonomic choice. Agents see the full module in one place — contract above, implementation below — without needing to navigate between a separate spec crate and an implementation crate.

### Test Scaffolding

At bootstrap, tests are written entirely in editable regions. Behavioral contracts appear as prose in the read-only `///` doc-comments above each item; the agent reads them and writes tests covering each. There is no spec → test machinery yet.

The evolution path layers in:

- **Doctests** for contracts simple enough to express as Rust assertions (`assert!(User::new("").is_err())`) — stencil emits them inside the read-only doc-block; `cargo test` runs them automatically.
- **Named test scaffolds** for contracts that need real test code — stencil emits `#[test] fn <name>() { todo!() }` stubs marked read-only at the signature level; agents fill bodies; `todo!()` in CI = unfinished work.
- **Prose-only** for invariants too complex to express mechanically — agent's responsibility to test by judgement.

This evolution lands once the bootstrap workflow has produced enough real contracts to know which mechanism each pattern needs.

### Spec Versioning vs. Semver

Kinographs evolve freely as a crate's spec is iterated; not every kinograph version is a published API change. **Spec version** is a separate, looser concept: a deliberate label saying "this kinograph version is the API contract for crate `foo` v0.4.0." Multiple kinograph revisions may roll up into one spec version.

This decoupling is intentional. It gives spec-authoring sessions room to refine without each save being a public commitment. The graduation event — "this is what we're publishing" — is a separate, deliberate act.

## Design Principles

1. **Public-API stability is structural, not aspirational** — The contract for crates under `crates/` lives in a separate, append-only system (kinora) and is rendered into code by a single trusted writer (stencil). It cannot be mutated as a side effect of writing implementation code.

2. **Convention-first, enforcement-later, with explicit migration paths** — Each piece of the mechanism starts with the lightest possible enforcement (markers, conventions, checklists) and has a documented path toward stronger structural guarantees. The migration is not a TODO — it is part of the design.

3. **Language-agnostic mixing (silp ethos)** — The same authoring model — read-only sections marked by comment conventions, interleaved with editable regions — works for any language with comment syntax. Stencil's bootstrap target is Rust; other languages are additive without redesign.

4. **API as kinograph; reuse via composition** — Shared, stable elements of the public surface live as atomic kinos that multiple crates' API kinographs reference. Crate-specific concepts live as logical-unit kinos. Granularity is mixed by intent, not by rule.

5. **Prototypes absorb design churn so `crates/` doesn't have to** — Free-form prototyping (anywhere outside `crates/`) is where API ideas are tried and discarded. By the time something graduates into `crates/`, the spec workflow's overhead is justified by the API's earned stability.

6. **Stencil is the sole author of read-only sections** — Read-only regions are produced only by stencil. Manual edits to those regions are a process violation. Editable regions are off-limits to stencil.

7. **Spec versioning is deliberate; kinograph evolution is fluid** — Kinographs may revise frequently during design; spec versions are intentional labels marking publishable API contracts. The looser coupling preserves the iterative nature of design sessions while keeping public API commitments deliberate.

## Bootstrap Sequence

1. **Define the spec kinograph format** — Reserve kinora event kinds (`kudo::api-spec` for atomic and logical-unit kinos, `kudo::api-kinograph` for the per-crate composition). Decide kinograph entry resolution (by content hash vs. by kino id) — this lands in the open questions until prototyping clarifies.

2. **Build stencil as the first spec-driven crate** — Stencil's own public API is defined as a kinograph; the first stencil binary is bootstrapped manually, then immediately rebuilt from its own spec (dogfooding).

3. **Implement the Rust target** — Stencil renders kinographs to Rust source files with read-only sections marked by comment conventions (specific syntax to be decided during bootstrap; one of the open questions).

4. **Pilot one published crate** — Apply the workflow to a single crate end-to-end. The natural candidate is the kinora-extracted shared crate (the content-addressed store + ledger primitives) that motivated the broader vision in the first place.

5. **Document the bootstrap learnings** — Capture what worked, what was awkward, and which evolution-path items now feel urgent.

## Post-Bootstrap Evolution

The bootstrap intentionally underbuilds; the evolution path lists what gets layered in once real usage clarifies the need.

- **Granularity refinement** — Discover the right shape for logical-unit kinos through prototyping; document patterns once they recur.
- **Test discipline** — Add doctest support, then named test scaffolds, then CI rules around `todo!()` density.
- **Enforcement upgrades** — Move from convention markers to hash-checked read-only regions; later, possibly to macro-expanded sections that fail to compile if mutated.
- **Additional language targets** — TypeScript, Python, etc., as crates needing them appear.
- **Public-API linter** — Detect drift between rendered read-only sections and actual exported items; warn on deprecations without proper marking.
- **Deprecation lifecycle** — A deliberate workflow for marking, communicating, and eventually removing public API items.
- **Changelog derivation** — Automatic changelog generation from kinograph version diffs, suitable for release notes.

## Open Questions

- **Marker syntax** — Which comment conventions delimit read-only sections? Reuse silp's, or define new (kinora-aware) markers?
- **Kinograph entry references** — Do API kinographs reference their entries by content hash (frozen forever) or by kino id (live, follows updates), or some mix? Likely depends on whether spec versions are expected to "follow latest" or "pin."
- **Spec version mechanics** — How is a spec version declared? A special kinograph kind? A ledger event? A tag-like overlay on top of kinograph history?
- **Where the kinora-extracted shared crate lives** — In this repo (kudo's `crates/`), in the kinora repo, or in moco? Affects the pilot-crate choice in the bootstrap sequence.
- **Test scaffold derivation rules** — When the hybrid test mechanism lands, which contract patterns map to doctests vs. named scaffolds vs. prose-only?
