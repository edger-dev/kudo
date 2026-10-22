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

### Every node is governed

`kudo-node` always runs its registry **governed**: an ad-hoc start is matched
on its exact argv against the node's rules before anything spawns. A matching
allow runs, a matching deny never spawns, and anything else **waits for a
verdict** from the console — allow once, allow always, deny once, deny always
— and is denied if none arrives in time. The "always" verdicts write the
node's standing rules (`standing-rules.styx` in the state directory), on top
of the seed. A governed start that carries an environment is refused.

- `KUDO_NODE_RULES` — the seed: a Styx file of exact argvs, e.g.

  ```
  allow ((df -h /) (uptime))
  deny ((rm -rf /))
  ```

  Unset, there are no rules, so every command waits for a verdict. A file
  that is named but missing or broken stops the node from starting.
- `KUDO_NODE_ALLOWED_ROOT` — every job's cwd must resolve inside it (default
  `/`).
- `KUDO_APPROVAL_TIMEOUT` — seconds a request waits for a verdict (default
  1800).

Every attempt, denials included, is appended to `audit.styxl` in the state
directory.

Declared jobs pass the gate too — boot entries, `ensure`, named starts and
restart-policy respawns. Put their argvs in `KUDO_NODE_RULES`, or each one
waits for a verdict (the node's boot line counts those as *waiting*).

Worker panes are the one start that does not pass the gate: the node offers a
`kudo-worker` connector whose `start_worker` takes only a context id, a repo
name and a lease, checks their shapes, and composes `kudo-shim worker …`, its
environment and its terminal itself (audited as *vouched*). The pane starts in
`/tmp`, or in the allowed root when that does not hold `/tmp`.

The standing rules and the audit live where the node's own jobs, running as
the same user, can write: until governed jobs run as another user, the seed
(read-only, from the Nix store) is the only rule source a job cannot touch.

**`kudo-shim`** — `KUDO_HUB_ADDR`: required for a session that only offers
the job tools; optional for a worker session (`KUDO_CONTEXT` set), which dials
the hub when a job tool first needs it and serves its worker tools without
one. The context store's mirror: see [The context store
mirror](../contexts/mirror.md).

## Checking it works

```console
$ kudo-node 127.0.0.1:7777
boot: started 1 node job(s)
kudo-node NodeId("alpha") link LinkLabel("stable") dialing hub at 127.0.0.1:7777 …
```

Then, from a workspace, `nodes` should list the machine and `job_list` should
show the boot job owned by `[system]` — running if its argv is in the seed
rules, pending a verdict if not. Restart the daemon and run `job_list`
again: the same jobs, the same ids, the same ports. If they vanished,
`MOCO_DIR` is not set.
