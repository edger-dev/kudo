# Introduction

A composable platform for human-agent workflows.

Kudo assembles independently-developed components into things a person and an
agent can both use. It is a **composition root**: the parts are built elsewhere
and know nothing about each other, and kudo is where they are wired together
into something that runs.

## What is here today

**[The process manager](./process-manager/overview.md)** — a machine-global
supervisor for the processes around your work. Dev servers, watchers, builds and
checkers get declared in a manifest checked into the repo, started by whoever
needs them, and kept alive independently of the session that asked. It answers
three questions nothing else was answering: *where is that server running*,
*what is wedging this machine*, and *did it compile* — that last one cheaply
enough that an agent will keep asking.

## How the pieces fit

Engineering intent lives in the [kinora](https://github.com/edger-dev/kinora)
ledger rather than in prose: each decision is an atomic spec with what was
chosen, what was rejected, and why. The code cites the specs it realizes. These
documents are the *user-facing* view; when you want to know why something is the
way it is, the spec is the answer, and
[RFC-0006](./rfcs/rfc-0006_component-boundaries.md) explains how work is split
across repositories.
