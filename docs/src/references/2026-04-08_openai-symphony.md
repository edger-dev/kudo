# OpenAI Symphony: Deep Research & Lessons for Kudo

## What Symphony Is

Symphony is an open-source orchestration daemon released by OpenAI in March 2026 under the Apache 2.0 license. Built in Elixir (~258 lines) on the Erlang/BEAM runtime, it connects an issue tracker (Linear) to coding agents (Codex), turning project management tickets into autonomous "implementation runs."

The tagline captures its philosophy: **"manage work instead of supervising coding agents."**

The reference implementation is Elixir, but the project ships as a language-agnostic specification (`SPEC.md`) that invites reimplementation in any stack.

---

## Core Architecture

### The Poll-Dispatch-Execute Loop

Symphony is fundamentally a **scheduler/runner**, not a workflow engine. Its operational cycle:

1. **Poll** — Fetch eligible issues from Linear on a fixed cadence (default: 30 seconds)
2. **Filter** — Check state eligibility, blocked-by dependencies, label filters
3. **Dispatch** — Create an isolated workspace, build the prompt, launch the agent (bounded by concurrency limits)
4. **Monitor** — Stream agent updates, track session metrics
5. **Reconcile** — Each tick, check if issue state changed externally (e.g., cancelled by a human)
6. **Complete** — Agent finishes at a handoff state (e.g., "Human Review"), workspace is preserved

### Six-Layer Architecture

Symphony separates concerns into portable layers:

| Layer | Responsibility |
|-------|---------------|
| **Policy** | `WORKFLOW.md` — YAML front matter for config + Markdown body as the agent prompt template |
| **Config** | Parses front matter into typed settings; handles defaults, env var indirection, path normalization |
| **Orchestrator** | Polling loop, issue eligibility, concurrency, retries, reconciliation — owns the in-memory runtime state |
| **Workspace** | Filesystem lifecycle, workspace preparation, coding-agent protocol — maps issue IDs to workspace paths |
| **Tracker** | API calls and normalization for issue tracker data (Linear GraphQL) |
| **Observability** | Structured logs + optional status surface for operator visibility |

### The WORKFLOW.md Contract

This is the most distinctive design choice. A single in-repo file serves as both configuration and agent instructions:

```yaml
---
tracker:
  kind: linear
  project_slug: "..."
workspace:
  root: ~/code/workspaces
  hooks:
    after_create: |
      git clone git@github.com:your-org/your-repo.git .
agent:
  max_concurrent_agents: 10
  max_turns: 20
---
You are working on a Linear issue {{ issue.identifier }}.
Title: {{ issue.title }}
Body: {{ issue.description }}
```

Key properties:
- **Version-controlled with the code** — agent behavior evolves with the codebase
- **Hot-reloadable** — file watcher detects changes, re-parses, re-validates, swaps live config without restart
- **Templated** — issue context is injected via Mustache-style variables
- **Hooks** — lifecycle hooks (`after_create`, `before_run`, `after_run`, `before_remove`) are arbitrary shell scripts

### Workspace Isolation

Each issue gets its own directory with:
- Its own git clone
- Its own MCP (Model Context Protocol) config
- Its own credentials
- Complete isolation from other running agents

This prevents cross-contamination — one agent's context never leaks into another's.

### Multi-Turn Continuation

On the first turn, the agent receives the full rendered prompt. On subsequent turns, it gets a minimal continuation — "you're still working on MT-123, keep going." The workspace persists between turns, so the agent sees its prior commits, partial code, test results. Up to 20 turns by default, configurable.

### The Workpad Pattern

Agents maintain a persistent "workpad" comment on the Linear issue — a running log of progress, decisions, and validation results. This serves as both observability for humans and context continuity for the agent across turns.

### Proof of Work

Before handing off to human review, the agent must demonstrate:
- CI/tests passing for the latest commit
- Acceptance criteria validated
- PR feedback sweep complete (no actionable comments remaining)
- PR checks green, branch pushed, PR linked to issue
- Required metadata present (e.g., `symphony` label)

---

## The "Harness Engineering" Philosophy

Symphony doesn't exist in isolation — it sits atop OpenAI's broader "harness engineering" philosophy, which fundamentally reframes the developer's role.

### Core Insight

> When something failed, the fix was almost never "try harder." The question was always: "what capability is missing, and how do we make it both legible and enforceable for the agent?"

### Key Principles

**1. Engineers become environment designers.** Instead of writing code, engineers design the environment — repo structure, linter rules, merge gates, documentation layout — so agents can operate independently.

**2. AGENTS.md as table of contents, not encyclopedia.** OpenAI tried a monolithic 800-line instruction file and it failed. Context is scarce — a giant file crowds out the task. They switched to a ~100-line AGENTS.md that serves as a map, pointing to a structured `docs/` directory. Progressive disclosure: agents start with a small entry point and discover context as needed.

**3. If agents can't see it, it doesn't exist.** Slack threads, Google Docs, hallway conversations — all invisible to agents. Knowledge must be encoded as versioned, discoverable artifacts in the repo.

**4. Enforce architecture mechanically.** Custom linters and structural tests enforce invariants. Background "garbage collection" agents scan for drift and open fix-up PRs. Human taste is captured once, then enforced continuously.

**5. Agent-to-agent review loops.** Codex reviews its own changes, requests additional agent reviews, iterates until satisfied. Humans are escalated only when judgment is required.

**6. Boring tech is better.** Technologies that are composable, API-stable, and well-represented in training data are easier for agents to model. Novelty is expensive in an agent-first world.

### Results

OpenAI's Harness team built a product with **zero lines of manually-written code** over five months: ~1 million lines of code, ~1,500 PRs, 3.5 PRs per engineer per day with a 3-person team (later 7). Even the initial `AGENTS.md` was written by Codex.

---

## What Kudo Can Learn

### 1. The WORKFLOW.md Pattern → Kudo's Agent Policy Files

**Symphony's insight**: Agent behavior should be version-controlled alongside the code it operates on. Configuration and prompt live in one file. Hot-reloadable.

**For Kudo**: Each moco app (kinora, hub, etc.) could have an analogous policy file — a composable agent contract that defines:
- What the agent is authorized to do
- What tools/capabilities are available
- What the handoff criteria are
- How to template context into agent prompts

This maps naturally to Kudo's vision of treating agents and humans as equal workflow participants. The policy file IS the contract between them.

**Specific idea**: Instead of `WORKFLOW.md`, Kudo could use a **kino-based policy** — the agent contract itself is a kino in kinora, with typed links to the project, tools, and constraints. This would be uniquely Kudo: the contract is knowledge-managed by the same system it governs.

### 2. Workspace Isolation → Per-Agent Kino Ownership

**Symphony's insight**: Each agent gets an isolated workspace. No cross-contamination.

**For Kudo**: This strongly validates the "Project-as-Agent" approach (Approach B) you've already identified. Each agent maintains autonomy over its own kinos. The message-passing model ensures agents don't directly mutate each other's state — they communicate through messages, just as Symphony agents communicate only through the issue tracker and git.

**Reinforcement**: Symphony's isolation is purely filesystem-level (directories). Kudo's architecture with per-project data ownership and SurrealDB as working memory can offer richer isolation — each agent's kinograph is its own namespace, with provenance tracking built into every kino.

### 3. The Workpad Pattern → Kino-as-Workpad

**Symphony's insight**: A persistent, agent-maintained progress log on the issue tracker provides both observability and context continuity.

**For Kudo**: This maps directly to kinora's append-only versioning model. An agent's "workpad" isn't just a comment — it's a chain of kinos recording decisions, validation outcomes, and state transitions. Because kinos have mandatory provenance, you get richer audit trails than Symphony's flat comment.

**Specific design implication**: The workpad concept validates kinora's append-only model as the right choice. Agents should never overwrite their reasoning — they should append new versions. This gives human reviewers (and other agents) a complete decision trail.

### 4. Harness Engineering → Kinora as the Harness

**Symphony's insight**: The repository itself is the agent's interface. Structure, docs, linters, tests — these ARE the control surface.

**For Kudo**: Kinora's structured knowledge base IS the harness. The insight about progressive disclosure (AGENTS.md as table of contents, not encyclopedia) maps perfectly to kinora's composition model:
- A top-level kino serves as the entry point (the "table of contents")
- Typed bidirectional links enable agents to discover context as needed
- Emergent structure over folder hierarchies means agents navigate semantically, not by path

**Key lesson**: OpenAI found that monolithic instruction files fail. Kinora's atomic kino model with typed links is architecturally aligned with this discovery. Each kino is a small, focused unit of context — exactly the progressive disclosure pattern that worked for OpenAI.

### 5. Hot-Reloading → Moco's Plugin Architecture

**Symphony's insight**: WORKFLOW.md is watched for changes and applied without restart. Elixir/BEAM's hot code reloading means you don't stop running agents when updating policy.

**For Kudo**: Moco's plugin-based app runtime should support similar hot-reloading of agent policies. When a policy kino is updated in kinora, running agents should be able to incorporate the new policy without restarting their workflow.

**Advantage over Symphony**: Symphony's hot-reload is file-system-level watching. Kudo could do this at the knowledge-graph level — when a policy kino gets a new version, the SurrealDB working memory can propagate that change to active agents via subscription/notification.

### 6. Proof of Work → Human Review as First-Class Workflow

**Symphony's insight**: Agents must demonstrate verifiable evidence before handing off. This isn't optional — it's structural.

**For Kudo**: This validates kinora's "human review as first-class workflow" principle. The proof-of-work pattern should be generalized beyond CI/tests:
- For knowledge work: provenance chains, source citations, confidence scores
- For creative work: draft-review-revision cycles tracked as kino versions
- For engineering work: CI results, test coverage, linter output (à la Symphony)

The handoff state concept ("Human Review" rather than "Done") is particularly important — agents should know that their job might end at a review gate, not at completion.

### 7. Status-Driven FSM → Typed State Machines for Kudo Workflows

**Symphony's insight**: Issues move through states (Todo → In Progress → Human Review → Merging → Done), and agent behavior is routed by current state.

**For Kudo**: Workflow states should be first-class in the kudo platform. A composable state machine where:
- States are defined per workflow type
- Transitions have preconditions (proof of work)
- Agents and humans both trigger transitions
- State changes are observable events that other components can react to

This is more general than Symphony's Linear-specific FSM — Kudo should make the state machine itself composable and pluggable.

### 8. Observability → Tsui as the Observability Surface

**Symphony's insight**: Operators need structured logs and human-readable status views. Without observability, you can't manage agents at scale.

**For Kudo**: Tsui's role as the generic GUI becomes critical here. The "agent monitor" app mentioned in tsui's design should provide:
- Real-time view of active agent workflows
- Kino-level audit trails (what did the agent produce, when, with what provenance)
- Workpad-style running logs for each active workflow
- State machine visualization showing where each workflow is

### 9. Spec-First Design → Kudo's Composable Specification

**Symphony's insight**: The specification (`SPEC.md`) is language-agnostic and explicitly invites reimplementation. "Tell your favorite coding agent to build Symphony in a programming language of your choice."

**For Kudo**: This is a powerful distribution strategy. Kudo's platform spec could be published as a composable specification that other tools can implement against. Hub's role as the "agent and developer gateway" could include publishing machine-readable specs that agents can consume to understand how to participate in Kudo workflows.

---

## Key Differences: Where Kudo Goes Further

| Dimension | Symphony | Kudo |
|-----------|----------|------|
| **Scope** | Software engineering only | All knowledge work |
| **Agent model** | Single agent per issue | Composable multi-agent with message-passing |
| **Knowledge management** | Files in a repo | Kinora's semantic knowledge graph |
| **State persistence** | In-memory + filesystem | SurrealDB working memory + markdown persistence |
| **Provenance** | Git commits + workpad comments | Mandatory provenance on every kino |
| **Versioning** | Git | Append-only kino versioning |
| **Composition** | One WORKFLOW.md per repo | Composable policies, typed links, mosaics |
| **GUI** | Terminal output / minimal dashboard | Tsui's full desktop environment |
| **Offline capability** | Not designed for it | SurrealDB WASM for browser/offline |
| **Review model** | Linear issue states | First-class human review workflow in kinora |

## What NOT to Borrow

1. **Linear-only coupling** — Symphony is tightly coupled to Linear as its tracker. Kudo should keep the tracker abstraction general from day one, not bolt it on later.

2. **Code-only focus** — Symphony assumes the output is always code (PRs, CI, git). Kudo's workflows produce knowledge artifacts (kinos), not just code.

3. **Flat policy files** — WORKFLOW.md is powerful but flat. Kudo can do better with composable, linked policy kinos that reference each other.

4. **No persistent database** — Symphony explicitly avoids requiring a database, using in-memory state with restart recovery. For Kudo's richer knowledge management, SurrealDB is the right choice — but the lesson about lightweight restart recovery is still valuable.

---

## Recommended Next Steps

1. **Design a kudo-native policy format** — Analogous to WORKFLOW.md but as composable kinos with typed links to tools, constraints, and handoff criteria.

2. **Prototype the workpad-as-kino-chain pattern** — An agent's running progress log as append-only kinos. This would be a compelling early demonstration of kinora's architecture.

3. **Define Kudo's state machine model** — Generalize Symphony's issue-state FSM into a composable workflow state machine that works for knowledge work, not just software tickets.

4. **Explore hub as the "tracker reader" equivalent** — Hub's role in receiving external work (from issue trackers, communication tools, etc.) and dispatching to agent workflows is directly analogous to Symphony's polling loop.

5. **Apply harness engineering to Kudo repos themselves** — Structure the edger-dev repos with progressive-disclosure documentation (à la AGENTS.md as table of contents) so agents can contribute to Kudo's own development.
