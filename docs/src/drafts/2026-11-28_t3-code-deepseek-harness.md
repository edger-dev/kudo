# Draft — T3 Code and DeepSeek Harness, and what they mean for kudo

- **Status**: Draft (working notes, not yet a kinora spec/task)
- **Created**: 2026-11-28
- **Scope**: the console (next big feature — smooth mobile-driven development) and the kinora index/design

Two analog products, researched to sharpen kudo's own plan. They sit at opposite
ends of one axis — **how much of the agent you build vs. borrow** — and kudo
deliberately sits between them.

## The two products

**T3 Code** (`pingdotgg/t3code`, ~50k stars) — a *control plane for coding
agents*, not a model. "It drives the agents you are already signed in to."
- Core abstraction: the **thread**, isolated by a **git worktree**.
- Submission *is* the git **diff → commit → PR**; review is "one button."
- Remote/mobile via LAN, Tailscale, or a tunnel (native iOS/Android apps).
- Top-level UI is an **inbox of unfinished work**, not a file browser.

**DeepSeek Harness** (`deepseek-ai/deepseek-harness`, `dsh`, ~288k stars) — an
agent *runtime*. "**Model + Harness = Agent**": the model is the brain, the
harness is the execution nervous system.
- "**Everything is a plugin**" (Cordis): models, tools, sessions, sandboxes,
  storage, loops, scheduling, UI are all swappable.
- **Append-only trajectory log** with recovery/fork/replay.
- Delegates *out* to external CLIs (Codex, Claude Code) via `subagent_oar`.

## The axis, and where kudo sits

| | T3 Code | DeepSeek Harness |
|---|---|---|
| Builds | coordination *surface* | execution *runtime* |
| Borrows | the entire agent (wraps CLIs) | the model (a plugin) |
| Persistence | git worktree/branch | per-session trajectory |
| Submission | git diff/commit | session/trajectory |

Kudo's ledger already placed it on the **T3 side**: *a worker is the ordinary
interactive agent CLI in a pane; its pull loop uses only the CLI's own hooks.*
That is validated by T3's traction.

But kudo builds a **third thing neither has**: the durable, semantic,
human+agent-shared **context** — resume block (`where/next/established/open`)
rewritten by typed submissions with evidence, plus a write-once trace, surviving
across sessions *and* repos *and* humans. T3's memory dies with the worktree;
DSH's memory dies with the session. Kudo's context is the gap both leave.

## Takeaways for the console (mobile-driven development)

1. **Inbox-first top level.** T3's sidebar is "unfinished work first." Kudo has
   the queue, due-checks, and the approval queue as *data*, but the console's
   first screen is panes over one context. → The top level should be *outstanding
   work* (waiting approvals, due items, blocked leases, open items), with the
   context as the drill-down.
2. **Zero-setup phone access is a product requirement, not a nice-to-have.** T3
   ships native apps + a tunnel and it drove adoption. Kudo's phone story today
   is "SSH tunnel, documented" — developer-grade. → Treat "drive from the phone
   with no setup" as a first-class requirement for the console work.
3. **A trajectory/timeline view answers "what is it doing right now" on a phone.**
   DSH's trajectory view (timeline, tool calls, per-call payload/timing) is the
   mobile-friendly shape for inspection. Kudo already writes the trace; it needs
   a *timeline* render, not just a head + list.
4. **The surface itself should be model-configurable.** T3: "the whole product
   is a prompt." Kudo's console is a fixed set of panes keyed by kind, with the
   arrangement as one value. → Allow the agent to *shape the surface* (layout,
   which panes, what a view answers), not just produce content inside it.

## Takeaways for kinora / the index

The RFC-management question surfaced here: RFCs live as plain markdown
(`docs/src/rfcs/`), deliberately **not** kinos, because kinos are meant to be
**atomic — one kino = one concept**. The atomic decisions inside an RFC are
*carved* into kinora specs, each back-linking "Carved from RFC-0006". The carve
step is **manual prose**, not a resolved edge:

- No query answers "has every decision in RFC-0006 been carved into a spec?"
- The RFC stays `Status: Draft` forever; the operative truth is the carved
  specs + code, so the RFC can silently drift from what was actually decided.

The **index** is the natural fix — a mechanical, mostly-LLM-free interface over
the ledger that can answer those questions deterministically and reconcile
RFC ↔ spec drift. In the landscape frame, the index is kudo's "make the harness
invisible" move: both competitors hid their substrate (T3 = just git; DSH =
plugins configured conversationally), while kudo's substrate (the kinora
ledger) is currently the most manual, visible part of the system.

## Next work implied here

1. **Kinora index + design** — the mechanical ledger interface, and with it the
   RFC ↔ spec reconciliation (atomicity is the reason RFCs aren't kinos; the
   index is what makes the carve/back-link a resolved edge instead of prose).
2. **Console as the next big feature** — mobile-driven development, using the
   four takeaways above as design input.

## Sources

- [T3 Code — GitHub](https://github.com/pingdotgg/t3code)
- [T3 Code — product page](https://gildhq.com/products/t3-code)
- [DeepSeek Harness — official site](https://www.deepseek.com/en/harness/)
- [DeepSeek Harness — GitHub](https://github.com/deepseek-ai/deepseek-harness)
- [subagent_oar unified CLI delegation](https://news.qiniu.com/archives/1793240013852)
- [Why agent frameworks are evolving into operating systems](https://36kr.com/p/4077516189718150)
