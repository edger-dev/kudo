# The console

`crates/kudo-console` is kudo's human surface: a Dioxus fullstack app. Its
server links the context engine (with this machine's mirror) and the hub
client; the page calls it through server functions. Three panes — *where*
(nodes, contexts, the workers under them), the selected context's **head**,
its **trace** — and a box to enqueue an ask under the selected context.

## Running it

In the dev shell (`nix develop`), from the repository root:

```
dx serve -p kudo-console --web --fullstack      # build, run, live-reload
```

or run a built server directly:

```
dx build -p kudo-console --web --fullstack --release
cd target/dx/kudo-console/release/web && ./server
```

Use the release build for anything reached over a network: the debug build's
WebAssembly is about 50 MB, and until it loads the page shows but no button
works. The release build's is about 2.4 MB.

It reads the same environment as `kudo-shim`: `KUDO_CONTEXT_STORE` (or the
default store), `KUDO_CONTEXT_MIRROR` and `AWS_*` for a mirror, and
`KUDO_HUB_ADDR` for nodes and live updates. It listens on `IP`:`PORT`,
**`127.0.0.1:8080` by default**.

## It has no login — keep it on localhost

v1 has **no authentication**: anyone who can reach the server can read every
context and enqueue asks that workers will act on. It refuses what another web
page could make your browser send (a write that is not same-origin JSON), but
nothing stops a person on the network. So:

- leave `IP` unset (localhost), and reach it from another machine through an
  SSH tunnel — `ssh -L 8080:127.0.0.1:8080 <host>`, then open
  `http://127.0.0.1:8080`;
- never bind it to a shared or public address.

## Starting a worker, and taking it over

Under each context, **start a worker on <node>** starts `kudo-shim worker`
for that context as a terminal job on that node (in the context's repo there,
when it declares one — otherwise in `/tmp`, where the agent CLI first asks to
trust the folder). The console picks the worker's lease, records the pane,
and selects it: the **terminal** pane shows its screen (re-read every second)
and takes typing — a line with Enter, or Enter / Ctrl-C / Esc alone. A worker
whose CLI has not started its session yet shows as *starting*, and can be
taken over all the same — which is how you answer a startup prompt.

Only panes the console started can be typed into, and only by the console
(the node refuses input from anyone else). A node too old to start a worker
pane is refused, and the job it started is stopped.

To reach the console by a name other than localhost (through a tunnel that
keeps a hostname), list it in `KUDO_CONSOLE_HOSTS` (comma-separated); any
other `Host` is refused, which also defeats DNS rebinding.

Behind an ingress that rewrites `Host` (the proxy's name reaches the console,
while the browser's `Origin` stays the public one), also list the public
origin in `KUDO_CONSOLE_ORIGINS`. For yolo's ingress:

```sh
PORT=8080 KUDO_CONSOLE_HOSTS=8080.yolo.incus \
  KUDO_CONSOLE_ORIGINS=https://yolo-8080.pc.yjpark.org ./server
```

The console still binds 127.0.0.1, where yolo's Caddy forwards. It has no
login, so anyone the ingress lets in can use it.

## Live updates

The server subscribes to the hub's `kudo/` events and the page long-polls a
version they bump, so it re-reads only when something moved (and at least
every 25 seconds). A hub without the event channel (older than the
`828a6b0` revision) still serves nodes; the page then refreshes on that
25-second beat.
