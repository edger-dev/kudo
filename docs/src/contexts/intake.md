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

## Native WSS through Cloudflare

A remote session can use `hub_edger.yjpark.org` directly without running
`cloudflared` in its Incus container. On the home hub, keep the existing TCP
listener and add a local WebSocket listener:

```sh
HUB_WS_ADDR=127.0.0.1:15401
HUB_AUTH_TOKEN=<same-token-as-everywhere-else>
```

Configure the `hub_edger.yjpark.org` published application to forward to
`http://127.0.0.1:15401`. Cloudflare terminates public TLS and forwards the
WebSocket upgrade. The hub's WebSocket listener itself remains plain and local;
home nodes can continue dialing its TCP listener.

In the sending Incus container:

```sh
KUDO_HUB_ADDR=wss://hub_edger.yjpark.org
HUB_AUTH_TOKEN=<same-token-as-home>
```

Home `kudo-node` instances can also dial a VPS-hosted hub directly with
`HUB_ADDR=wss://hub.example.com`; registration, callbacks, token authentication,
and reconnect backoff are shared with TCP. Cloudflare Access service-token
headers are currently supported by the sending shim, not by nodes. For nodes,
use a direct VPS TLS endpoint or a private route without interactive Access.

A home node can also stay on its private hub. Configure only that hub to dial
the public hub with `HUB_NAME=edger` and
`HUB_PEERS=hub=wss://hub.yjpark.org`. The private hub authenticates and
registers a reverse consumer lane over the same connection. The public hub then
sees the home node as `edger::<node-id>`, which is the `node` value for
`send_note`; it needs no return address or peer entry of its own. Edger sees
public nodes as `hub::<node-id>`. Routing remains one hop, and reconnects replace
the old registration safely. Older hubs without reverse registration still need
an explicitly configured return peer.

If Cloudflare Access protects the hostname, also configure a service token:

```sh
CF_ACCESS_CLIENT_ID=<service-token-client-id>
CF_ACCESS_CLIENT_SECRET=<service-token-client-secret>
```

The pair adds `CF-Access-Client-Id` and `CF-Access-Client-Secret` to the
WebSocket handshake. Both must be nonempty and set together. They supplement,
never replace, `HUB_AUTH_TOKEN`. No local `cloudflared access tcp` process is
needed for this WSS setup. A tunnel still configured as `tcp://` rather than an
HTTP WebSocket route continues to require the client-side forwarder.

## Provenance and retries

`send_note` requires `node`, `target`, `title`, `body`, and `attestation`.
Before setting `attestation=true`, review that the note contains no code and no
employer-specific information. `id` is optional: reuse it when retrying. The
tool generates a fresh id otherwise.

The tool derives provenance from `KUDO_INTAKE_AUTHOR`, `KUDO_INTAKE_MACHINE`,
and `KUDO_LEASE` where set, with the current user, node name, and `bare` as
fallbacks. Optional `author`, `machine`, and `session` arguments override them.
Creation and receipt timestamps are not part of the deduplication fingerprint.

## Starting implementation from the console

The console's inbox lists these notes with their body and provenance. Start
implementation creates a linked context with an allowed wake node and an ask to
decompose and execute the design in small serial steps. The note remains
unreviewed and is not made into a spec. Re-selecting it returns the existing
context. Contexts without an explicit wake node keep their previous behavior:
ordinary work waits for a person.
