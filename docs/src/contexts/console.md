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
and takes typing — a line with Enter, or a key alone (the arrows, Tab, Enter,
Esc, Ctrl-C; the arrows and Esc also work from the empty line field), sent in
order. A worker
whose CLI has not started its session yet shows as *starting*, and can be
taken over all the same — which is how you answer a startup prompt. One still
silent after ten minutes shows as *quiet* (stuck at a prompt, or dead), and
after a day it is no longer listed.

Only panes the console started can be typed into, and only by the console
(the node refuses input from anyone else).

A worker with nothing ready **waits** for the next item instead of ending: it
shows as `wait` in the *where* pane, and an item queued later is handed to it.
It looks locally every 3 s and syncs the mirror every 30 s. About every nine
minutes of waiting it replies one idle line (a cheap turn)
and waits again; after `KUDO_IDLE_LIMIT` seconds in a row with nothing
(default 7200, on the node) its lease ends. `KUDO_IDLE_LIMIT=0` ends it as
soon as nothing is ready. To take over a waiting worker, interrupt (Esc) and
type.

### Which nodes run workers, and how

`KUDO_CONSOLE_WORKER_NODES` (comma-separated node ids) limits where the
console starts workers: only those nodes are offered, and a start on any other
is refused. Unset, every node is offered.

Every node is governed, so the console does not start a worker with a
command line: it asks the node's `kudo-worker` connector for a worker on the
context, its repo there and a lease, and the node composes the rest (see
[Deployment](../process-manager/deployment.md)). A node too old to offer it is
refused — redeploy it.

Whether a worker's CLI asks before running tools is the **node's** decision,
never the console's: a node whose own environment has `KUDO_WORKER_BYPASS=1`
runs its workers with `--permission-mode bypassPermissions`. No caller can set
it: the worker's environment is composed by the node. Set it only on a
sandboxed node — yolo, whose outbound traffic goes through OneCLI:

```nix
services.kudo-node.environment.KUDO_WORKER_BYPASS = "1";
```

and run the console with `KUDO_CONSOLE_WORKER_NODES=yolo`.

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

## Answering the gate

Every node is governed, so a command no rule allows waits for **your**
verdict. Such jobs show at the top of the *where* pane, under **waiting for
you**, oldest first: the argv, the node and directory, the context whose
worker asked, the owner, and the minutes left before it is denied. The buttons
are the four verdicts — **allow once**, **allow always** (writes an exact-argv
rule on that node), **deny once**, **deny always** — and **edit, then allow**,
which runs your corrected argv instead (shell-quoted: quote an argument with spaces; an empty or unparsable edit is refused). The reply says what
the job then did; a verdict that arrives too late runs nothing, and says so.

Only the console's verdict counts: a node refuses one from anyone else. A node whose queue could not be read is named above the queue (*queue unknown on …*), so silence there is not "nothing waiting".

## Live updates

The server subscribes to the hub's `kudo/` events and the page long-polls a
version they bump, so it re-reads only when something moved (and at least
every 25 seconds). A hub without the event channel (older than the
`828a6b0` revision) still serves nodes; the page then refreshes on that
25-second beat.
