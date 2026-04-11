# RFC-0002: Moco / Tsui / App Boundary

- **Status**: Draft
- **Created**: 2026-04-10
- **Project**: Kudo (edger-dev/kudo)
- **Components**: Moco, Tsui, App Layer

## Summary

Kudo applications span three layers: Moco (data runtime), Tsui (GUI environment), and the App (domain logic). Drawing the boundary between these layers is the most consequential architectural decision in the platform, because a misplaced boundary forces every application built on Kudo to work around it. This RFC describes the boundary model, uses a JSON log viewer as a tracer bullet to stress-test it, and documents the open questions that must be resolved through prototyping.

## Motivation

The frustrations that motivate Kudo's design are not feature gaps in any specific application — they are symptoms of GUI architectures that force developers to make decisions that should belong to users:

- An application chooses a horizontal split layout. A user on a vertical monitor cannot change it.
- An application provides tabbed views. A user who wants side-by-side comparison cannot get it.
- An application renders content in one way. A user who wants to see the same data differently (pretty-printed JSON vs. tree view, word-wrapped vs. raw) must find a different tool.
- An application captures text on screen that the user cannot select, copy, or search — the content is visible but not accessible.
- An application runs on one monitor. A user with two monitors cannot spread the workspace across them.

These are not edge cases. They represent a fundamental inversion: the developer's design constrains the user's workflow, rather than the user's intent shaping the interface. Kudo's three-layer architecture exists to correct this inversion.

## The Three Layers

### Moco — Data Runtime

Moco manages immutable, content-addressed data (see RFC-0001). From the boundary perspective, Moco's role is:

- Store and serve content-addressed blobs
- Maintain reference structures (ordered lists, indices, relationships)
- Host thin apps that produce and consume data
- Provide query and subscription interfaces

Moco knows nothing about presentation. It does not know what a "panel" is, what a "tab" is, or how data should be displayed. It knows about data identity (hashes), data relationships (references), and data availability (what exists and when new data arrives).

### Tsui — GUI Environment

Tsui is a desktop environment, not a widget toolkit. It provides the user with control over how they see and interact with data. Tsui's responsibilities include:

- **Layout management** — splits (horizontal, vertical), panels, multi-monitor spanning. The user controls layout; applications do not dictate it.
- **View multiplexing** — the same data can be shown in multiple views simultaneously. Two panels can display the same content-addressed blob with different renderers.
- **Tab and workspace management** — opening, pinning, arranging, comparing views. These are Tsui primitives, not per-application features.
- **Universal text interaction** — any text visible in Tsui is selectable, copyable, and searchable. This is an environment-level guarantee, not something each app must implement.
- **View model management** — presentational transformations (pretty-printing, syntax highlighting, word wrapping) that are ephemeral and per-view.

Tsui accesses Moco data through Moco's query interface. Transformations that are presentational stay local to Tsui as view models. Transformations that produce reusable, deterministic results may themselves be stored back into Moco as derived data.

### App Layer — Domain Logic

An app brings domain knowledge. For the JSON log viewer, the app knows:

- That the blobs are JSON
- That there is a "level" field with meaningful values like "error" and "info"
- What views are meaningful for this data (pretty-printed JSON, structured tree, raw text)
- What filters are semantically useful

The app registers capabilities with Moco and Tsui: "I understand this kind of data, and here are the ways I can present it." But the app does not control layout, view arrangement, or user interaction patterns — those belong to Tsui.

## Tracer Bullet: JSON Log Viewer

The JSON log viewer serves as a tracer bullet to validate these boundaries. It exercises a realistic slice of all three layers:

### Data Flow

A watcher Moco app monitors a JSONL log file. When new lines appear, it parses them, content-addresses each event as a blob, and maintains an ordered reference list of event hashes in Moco. Duplicate events (identical content) are stored once and referenced multiple times.

A viewer app reads these blobs and reference lists through Moco's query interface. Tsui renders views of the data according to user-controlled layout.

### Scenarios to Validate

**Scenario: Side-by-side JSON and tree view of the same event.** The user selects an event from a list, then splits the view to see both pretty-printed JSON and a structured key-value tree. Both views reference the same content-addressed blob. Tsui manages the split and instantiates two view renderers. The app provides both renderers. Moco serves the same blob to both.

**Scenario: Vertical split on a vertical monitor.** The user rotates the split orientation. This is purely a Tsui operation. Neither Moco nor the app is involved or aware.

**Scenario: Multi-monitor workspace.** The user moves the event list to one monitor and the detail view to another. Tsui manages window placement across monitors. The app does not know how many monitors exist.

**Scenario: Filtering error events.** This is the hardest boundary question. See "The Filtering Problem" below.

**Scenario: Universal text selection.** The user wants to copy a specific JSON path from the tree view. Tsui guarantees that any rendered text is selectable and copyable, regardless of how the app's renderer produced it.

## The Filtering Problem

Filtering is the most instructive boundary question because it genuinely spans all three layers:

### Three Levels of Filtering

**Content-level filtering (Moco)** — operations on raw content without schema knowledge. "Give me all blobs from this reference list that contain the byte sequence `error`." This is fast, cacheable, and the result is itself a new immutable reference list. No parsing, no schema, no domain knowledge.

**Structural filtering (Library)** — schema-aware but application-independent. A JSON-aware filter library can parse any JSON blob and evaluate predicates on its fields. This library is usable from APIs and GUIs alike. The log viewer app uses this library, but the library itself is reusable across any JSON-based application.

**Interactive filtering (Tsui)** — the user clicks a value in the tree view and says "show me more like this." Tsui captures the interaction (user selected a value), the app translates it into a predicate (field X equals Y), and the predicate runs through either the library or Moco to produce a filtered result set.

### Composition Question

The open question is how these levels compose. Can a Tsui interaction produce a library-level predicate that runs through Moco and yields a cached, content-addressed result list? The ideal flow would be:

1. User clicks a value in Tsui (Tsui concern)
2. App translates the click into a filter predicate (App concern)
3. Predicate evaluates against Moco data, producing a filtered reference list (Moco + Library concern)
4. Filtered reference list is itself stored as immutable Moco data (Moco concern)
5. Tsui displays the filtered list, which can be further filtered, split, or compared (Tsui concern)

### Approach Under Consideration

Reusable filtering logic as a library, usable from both APIs and GUIs. Schema-specific configuration or a DSL layer sits on top of the library, providing domain-aware filtering without baking domain knowledge into the core. Dynamic scripting may bridge the gap between generic structural filtering and domain-specific queries.

This approach is deliberately not finalized. The tracer bullet's purpose is to try multiple approaches with the log viewer and evaluate the tradeoffs before committing.

## Design Principles

1. **Users control layout, apps provide content** — No application should dictate how views are arranged, how many monitors are used, or whether panels are split horizontally or vertically. Layout is a user concern managed by Tsui.

2. **Same data, multiple views** — Any content-addressed blob can be rendered by multiple view renderers simultaneously. Tsui manages the multiplexing; apps provide the renderers.

3. **Text is always accessible** — Any text visible in Tsui is selectable, copyable, and searchable. This is an environment guarantee, not a per-application feature.

4. **Presentational transforms stay in Tsui; reusable transforms go to Moco** — Pretty-printing, syntax highlighting, and word wrapping are view-local. Parsing, indexing, and filtering may produce Moco data when the results are deterministic and reusable.

5. **Apps declare capabilities, Tsui orchestrates them** — An app says "I can render this data as a tree view." Tsui decides when and where to instantiate that renderer based on the user's workspace configuration.

## Open Questions

- What is the contract for an app to "declare" a view renderer? Is it a trait, a plugin interface, a message protocol?
- How does Tsui discover which renderers are available for a given content type?
- Should filtered result sets always be stored in Moco, or only when explicitly requested?
- How does the library approach to filtering interact with Moco's subscription model? (If a filter is active and new data arrives, who re-evaluates?)
- What is the minimum set of Tsui layout primitives needed? (Split, tab, pin, float — which are essential vs. which can come later?)
- How does universal text selection work with non-text renderers (images, diagrams, binary views)?
