# Deployment

Three binaries, and three quiet lines that are load-bearing.

## Binaries

| Binary | Role |
|---|---|
| `kudo-node` | The supervisor daemon. One per machine. |
| `moco-pty-holder` | Owns one `@Terminal` job's pty. Spawned by the daemon. |
| `kudo-shim` | The MCP server. One per workspace, launched by the agent. |

`moco-pty-holder` must be installed **beside** `kudo-node`, or named by
`KUDO_PTY_HOLDER`. The daemon says so at startup if it cannot find one.

## The unit

```ini
[Service]
ExecStart=/usr/local/bin/kudo-node
Restart=always

KillMode=process
Environment=MOCO_DIR=/var/lib/kudo-node
Environment=KUDO_PTY_HOLDER=/usr/local/bin/moco-pty-holder
```

Those last three lines look like clutter. They are the deployment, and each is
asserted by a test because each is only noticed by its absence, at the worst
possible moment.

**`KillMode=process`.** systemd's default signals every process in the unit's
cgroup — which is every job the daemon supervises. A routine `systemctl restart`
would take down the servers and builds the supervisor exists to keep alive, so
*a job outlives any one client* would be false exactly while deploying a new
version of the thing responsible for keeping them alive. `process` signals only
the daemon; its jobs keep running and are re-adopted when it returns.

**`MOCO_DIR`.** The state directory must be **stable across restarts**. The
default is a per-process directory — correct for a library, ruinous for a
daemon: a supervisor handed a fresh directory on every start re-adopts nothing,
boots nothing, and forgets every port it ever allocated. Nothing fails loudly;
it simply comes up empty.

**`KUDO_PTY_HOLDER`.** Without it, `@Terminal` jobs still run — they just lose
their live terminal whenever the daemon restarts.

## The node manifest

`$MOCO_DIR/moco-processes.styx` declares what *this machine* runs, independent
of any checkout. Entries with `autostart @Boot` start when the daemon does.

```styx
proc ({name housekeeping, argv (/usr/local/bin/sweep),
       cwd "/var/lib/kudo-node", autostart @Boot})
```

A node entry must declare an **absolute `cwd`** — there is no workspace root to
fall back on, and an ambient working directory for a system service is a bug
that gets blamed on something else later.

Boot is `ensure`, not `start`: re-adoption runs first, so a job the previous
daemon left running is left alone rather than started a second time alongside
itself. It also runs **before the hub is dialed** — these are the machine's own
services, and making them wait on a transport would tie a local concern to a
remote one.

## Environment

**`kudo-node`** — `MOCO_DIR` (state directory), `NODE_ID` (defaults to the
hostname), `NODE_LINK` (default `stable`), `HUB_ADDR` (or argv[1]),
`KUDO_JOB_CONNECTOR` (default `jobs-0`), `KUDO_PTY_HOLDER`, `MOCO_PORT_RANGE`
(default `10000-19999`).

**`kudo-shim`** — `KUDO_HUB_ADDR`, required.

## Checking it works

```console
$ kudo-node 127.0.0.1:7777
boot: started 1 node job(s)
kudo-node NodeId("alpha") link LinkLabel("stable") dialing hub at 127.0.0.1:7777 …
```

Then, from a workspace, `nodes` should list the machine and `job_list` should
show the boot job owned by `[system]`. Restart the daemon and run `job_list`
again: the same jobs, the same ids, the same ports. If they vanished,
`MOCO_DIR` is not set.
