# Kudo

A composable platform for human-agent workflows.

Kudo provides the foundation for humans and AI agents to work together on knowledge-intensive tasks. Rather than being a monolithic system, Kudo integrates purpose-built components that each handle a distinct concern, composing them into a smooth, unified experience.

## The Problem

Working with AI agents today means cobbling together disconnected tools with no shared substrate. Agents lack persistent knowledge, humans lack visibility into agent work, and cross-project coordination requires manual glue.

## Kudo's Approach

Each concern is handled by a focused component with a clear boundary. Kudo is the integration layer that makes them work as one.

## Components

- **[moco](https://github.com/edger-dev/moco)** — App runtime with a plugin architecture. Kudo systems are composed of multiple moco apps — each project may run its own kinora app, hub connector, and project-specific plugins. Moco provides lifecycle management and extensibility for all of them.

- **[tsui](https://github.com/edger-dev/tsui)** — Generic GUI framework for moco apps. Provides web and desktop interfaces, driven by data rather than custom UI code.

- **kinora** — Agent-first knowledge system. Tasks, docs, and communications are stored as atomic semantic units (kinos) with provenance tracking, managed in Git as `.kinora/` within each project. Both humans and agents are first-class authors.

- **[hub](https://github.com/edger-dev/hub)** — Agent and developer gateway. Maintains long-lived connections to project coordinators running in sandboxed containers, routes messages between them, and exposes agent state to external users.

- **[jig](https://github.com/edger-dev/jig)** — Scaffolding and maintenance tooling. Used to bootstrap, update, and manage Kudo and its component projects. Not a runtime dependency.

## Principles

- **Each project owns its own data** — Kudo coordinates but never centralizes project state. Cross-project work happens through inter-agent communication, not a shared database.

- **Lightweight integration, not a framework** — Kudo doesn't change how other tools work. It's plumbing that connects purpose-built components.

- **Agent-first, human-supervised** — Agents are full participants with their own knowledge, sessions, and agency. Humans maintain visibility and control through review workflows and the hub's ingress.

- **Data drives UI** — Define your data in kinora, get GUI features from tsui automatically. The interface is a projection of the knowledge graph, not a hand-built dashboard.

- **Provenance everywhere** — Every piece of knowledge tracks who created it (human or agent), when, and why. Trust is built on transparency.

