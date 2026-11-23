# Remote inbox notes

An agent on another machine can deliver a reviewed text note into a repository's
Kinora inbox at home through two MCP tools:

- `send_note` delivers one note and returns `delivered kino://…` or
  `duplicate kino://…`.
- `intake_list` lists arrivals at or after an RFC 3339 time, oldest first.

Delivery is **online only**. `refused` means the receiver rejected the note and
says why. `unavailable` means the hub or receiver could not be reached and
accepted nothing. There is no offline queue: retry later, reusing the same note
id so a retry cannot create a duplicate.

## Authenticate the connection

Set `HUB_AUTH_TOKEN` on the hub, every node, every peer hub, and every shim or
console that dials it. Every connection must authenticate before registration,
topology reads, subscriptions, publication, or routing. Missing and incorrect
credentials are refused. An empty token is a startup error; leaving it unset
keeps existing behavior.

The token authenticates but does not encrypt traffic. Use a private or
separately encrypted network, never a public plaintext listener. It comes from
the environment, not a command line, and is not logged.

## Configure the home receiver

Enable the `kudo-intake` connector on the home `kudo-node` with
`KUDO_INTAKE_TARGETS`, a comma-separated list of aliases and absolute paths to
dedicated clean `main` checkouts:

```sh
KUDO_INTAKE_TARGETS=kudo=/home/yj/intake/kudo,kinora=/home/yj/intake/kinora
```

The receiver refuses unknown aliases, dirty or detached checkouts, changed
content under an existing id, and backend errors. A valid note becomes one
markdown kino in the target inbox with author, machine alias, session, creation
and receipt times, and `status::unreviewed=true`. Kinora compacts and the
receiver makes a local Git commit. It never pushes notes to GitHub; other
checkouts can pull from the local intake clone later. Deduplication still works
after triage moves a note out of the inbox.

The default body limit is 256 KiB. Override it with
`KUDO_INTAKE_MAX_BODY_BYTES`. `KUDO_INTAKE_SENSITIVE_MARKERS` is a
comma-separated, case-insensitive denylist checked in titles and bodies.
Fenced and indented code blocks are rejected. These deterministic checks
supplement the sender's attestation; they cannot recognise every kind of
sensitive content.

## Provenance and retries

`send_note` requires `node`, `target`, `title`, `body`, and `attestation`.
Before setting `attestation=true`, review that the note contains no code and no
employer-specific information. `id` is optional: reuse it when retrying. The
tool generates a fresh id otherwise.

The tool derives provenance from `KUDO_INTAKE_AUTHOR`, `KUDO_INTAKE_MACHINE`,
and `KUDO_LEASE` where set, with the current user, node name, and `bare` as
fallbacks. Optional `author`, `machine`, and `session` arguments override them.
Creation and receipt timestamps are not part of the deduplication fingerprint.
