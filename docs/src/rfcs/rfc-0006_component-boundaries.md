# RFC-0006: Component Boundaries and Cross-Repo Intent

- **Status**: Draft
- **Created**: 2026-07-26
- **Project**: Kudo (edger-dev/kudo)
- **Components**: Moco, Hub, Kudo, Tsui, Kinora

## Summary

The kara prototype was a single repository, so placement was free and a spec sat
next to the code realizing it by construction. The rewrite spans six repos, and
both properties are gone: a design now lands in three places at once, and the
link between a contract and its implementation has to be written down or it does
not exist.

This RFC fixes the boundary model on **two axes**. *Layering* extends RFC-0005's
"no upward edges" across the whole component set. *Kind* sorts every repo into
**engine**, **transport**, or **face**, with one testable rule — **a face may
contain no logic a second face would need** — that decides the cases the layering
axis leaves ambiguous.

It then addresses the consequence the split creates: a contract may be
**declared** in one repo and **realized** in another, so the `// implements:`
citation becomes the only thing binding them. Today that citation cannot be
written across repos at all, which is the concrete gap this RFC opens for
resolution.

## Motivation

Three symptoms have already been hit in practice, all within one feature.

**1. One design, three repos.** The process-manager brief describes a supervisor
whose engine belongs in `moco`, whose agent face belongs in `kudo`, and whose
reachability comes from `hub`. Nothing said where the brief itself should live,
or where each of its thirteen specs should be carved. The answer taken — brief and
engine specs to `moco`, the shim spec deferred to `kudo` — was defensible but
ad hoc, and the next feature would have to re-derive it.

**2. A contract satisfiable only in another repo.** `job-durability-both-kill-vectors`
(a `moco` spec) requires a service-manager unit configured with
`KillMode=process`. Moco ships no daemon binary; the node daemon lives in `hub`.
So the spec is declared where the requirement is understood and can only be
realized where the packaging exists. Under a single repo this was invisible.

**3. The citation cannot cross a repo.** Code refs are unqualified today
(`// implements: argv-not-shell`) and kino ids are per-ledger. There is no form in
which `hub`'s packaging can cite the `moco` spec it satisfies, which means symptom
2 has no expressible answer.

Behind all three sits a quieter one: **kudo has two homes for intent and uses
only one.** `.kinora/config.styx` here declares `specs`, `tasks`, and `inbox`
roots that have never been created, while every decision lives as prose in
`docs/src/rfcs/`. Meanwhile `hub` and `moco` carve spec kinos with stable ids.
Cross-repo citation needs an id to point at; prose has none.

## Design

### Axis A — layering: a strict DAG with no upward edges

RFC-0005 established this for the storage stack (Moco ← Kinora ← Kura). It
generalizes to the whole set:

```
hub-protocol   — pure message and identity types; depends on nothing else here
     ↑                        ↑
   hub                      moco          — binds the published protocol so it is
 (transport)              (engine)          reachable; never hub's internals
                             ↑
                    kinora  ·  kura        — depend down on moco (RFC-0005)
                             ↑
                    kudo  ·  tsui          — present engines; nothing depends on them
```

The two edges that must never appear are **hub → moco** and **engine → face**.
Both are already contracted:

- `job-substrate-is-a-moco-cell-layer` — "the hub relays connectors and knows
  nothing of jobs, rule-sets, approvals, or audit."
- `hub-connector-owns-its-payload` — "the hub routes on the *addressing* fields
  only and never decodes, validates, or rewrites the payload."
- `hub-stable-protocol-boundary` — the hub presents a stable protocol; what sits
  behind it is swappable.

Layering answers "may A call B". It does **not** answer "which repo does this
file go in" when several are permitted — which is most of the time.

### Axis B — kind: engine, transport, face

| kind | holds | links | repos |
|---|---|---|---|
| **engine** | the capability itself — state, policy, lifecycle | no transport, no UI framework, no protocol SDK | `moco`, `kinora`, `kura` |
| **transport** | routing, reachability, topology | no domain semantics | `hub` |
| **face** | a thin per-consumer adapter — MCP shim, console, CLI | the engine it presents | `kudo` |

**The rule that makes "thin" testable:**

> **A face may contain no logic that a second face would need.** The first time
> two faces need the same thing, it moves into the engine — it is never copied.

This is the operational form of what the process-manager brief argued from
evidence: because its engine linked no transport and no UI, the supervisor was
re-hosted from a standalone daemon onto an existing one **with its core
untouched**, and the migration deleted more code than it added. Consumer-agnosticism
is not tidiness; it is the property that makes a component survive its host
changing.

A corollary worth stating because it has already paid: **host a capability on the
daemon that exists.** A second per-node daemon duplicates transport, liveness, and
deployment for nothing. If you are writing one, ask what the first is missing.

**Where Tsui sits.** Tsui is *not* a face under this taxonomy, and RFC-0002 is
right that it is "a desktop environment, not a widget toolkit". It is a fourth
kind — an **environment** that hosts faces rather than being one. RFC-0002's
three-layer model (Moco / Tsui / App) composes with this one: the App layer is
where a face lives when its consumer is a human at a GUI.

### Placement rules

Four rules follow, and together they decide every case encountered so far:

1. **A design brief lives with the engine it primarily describes.** Other repos
   cite it rather than copying it. (Brief 1 → `hub`, briefs 2 and 3 → `moco`.)
2. **A decision internal to one component is a spec kino in that component's
   ledger.** `argv-not-shell` belongs to `moco` and nowhere else.
3. **A decision binding two or more repos is an RFC in `kudo`.** This is already
   the de facto rule — RFC-0005 spans Moco/Kinora/Kura and lives here — and this
   RFC makes it explicit.
4. **Declaration and realization may live in different repos.** The spec belongs
   with the component that understands the requirement; the implementation
   belongs with the component that can satisfy it; the `// implements:` citation
   is what binds them. This is the rule that replaces co-location.

Rule 4 is the load-bearing one. In a single repo, a contract and its code were
held together by proximity. Split apart, **nothing holds them together except an
explicit reference** — so that reference has to be expressible, and it has to
survive renames.

### Cross-repo citation

Today a citation is a bare name resolved within one ledger:

```rust
// implements: argv-not-shell
```

The namespacing convention already exists elsewhere in the ledger — rule kinos
are named `jig::rust::no-unwrap-in-lib` — so the natural extension is to qualify
a cross-repo citation with its component:

```rust
// implements: moco::registry-is-node-state-on-disk
```

with an unqualified name continuing to mean "in this repo's ledger". That is
cheap, readable, and greppable.

It also sits in tension with kinora's own model, which says to reference a kino
by **stable id, never by title** — precisely so a rename does not break the link.
Within a repo that tension is tolerable because a rename and its citations are
fixed in one commit. Across repos they cannot be, which is exactly when the
stable id earns its keep. Resolving this is an open question below rather than a
decision here.

### Kudo's two homes for intent

RFCs and kinos are not competitors; they do different jobs:

- **An RFC is the narrative** — the motivation, the worked example, the
  alternatives weighed and why they lost. It is meant to be read start to finish.
- **A spec kino is the citable contract** — one atomic decision, a stable id,
  something code elsewhere can point at.

The proposal: **RFCs remain the record for cross-repo decisions; additionally,
any part of an RFC that code in another repo must cite gets a spec kino in kudo's
`specs` root, so it has an id.** Not every RFC paragraph needs a kino — only what
something else has to reference. This activates the roots kudo already declares,
without turning prose into a ledger.

## Design Principles

1. **No upward edges.** The dependency graph is a DAG; `hub → moco` and
   `engine → face` are the edges that must never appear.
2. **An engine links no transport, no UI framework, and no protocol SDK.** This
   is what lets it be re-hosted without change.
3. **A face holds nothing a second face would need.** When two need it, it moves
   down into the engine.
4. **The transport never inspects payloads.** Routing reads addressing fields
   only; encoding is private between a connector and its callers.
5. **Host a capability on the daemon that already exists.**
6. **Declaration and realization may live in different repos**; the citation, not
   proximity, is what binds them.
7. **Cross-repo decisions are RFCs in kudo; component-internal decisions are
   specs in the component.**

## Sequencing

1. Adopt the qualified citation form (`component::spec-name`) — convention only,
   no tooling change, and it makes symptom 2 expressible immediately.
2. Create kudo's `specs` root and carve the citable contracts from this RFC
   (principles 1–4 are the candidates; the rest are guidance, not contracts).
3. Resolve the name-vs-id question below before the first cross-repo citation is
   relied upon for anything mechanical.
4. Revisit whether `kinora resolve` should follow a cross-repo reference, which
   needs a component→repo map that does not exist yet.

## Open Questions

1. **Name or stable id in a cross-repo citation?** A name is readable and
   greppable; an id survives renames, which is the whole reason kinora's model
   prefers it, and cross-repo renames cannot be fixed atomically. A hybrid
   (`moco::registry-is-node-state-on-disk@a8f6cfd5`) is possible but verbose and
   would drift on every re-version.
2. **Should `kinora resolve` follow cross-repo references?** That requires a
   registry mapping a component namespace to a repo URL, and a policy for
   resolving against a checkout that may be absent or at a different revision.
3. **Where does packaging live?** The `KillMode=process` unit for moco's job
   substrate is realizable only where a daemon is packaged, which today is hub's
   `node` binary. Does packaging permanently belong to the transport repo because
   that is where the daemon is, or does moco eventually ship its own daemon and
   take it back?
4. **Does a face ever legitimately hold state?** The rule says a face is a thin
   adapter, and the process-manager brief is explicit that the MCP shim holds
   none. Client-side aggregation in the console (rollups, sparkline rings)
   is state that exists in no engine — is that a violation, or the boundary
   working as intended because no *second* face needs it?
