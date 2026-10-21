# Developing kudo

Each layer is developed against the layer below it, or against the stand-in
that layer itself ships — no full stack for daily work. A full stack is
assembled only for the occasional end-to-end check.

## Stand-ins, and the real thing behind a variable

| Layer below | Stand-in (default) | The real thing (opt-in) |
|---|---|---|
| Object store (context store mirror) | `memory://`, `file:///dir`; MinIO via `scripts/dev-s3` | `KINORA_REMOTE_S3_TEST=s3://…` (+ `AWS_*`) runs kinora-remote's round trip against it |
| Hub (job tools) | the hub crate's in-process `HubListener` + `ServedHub` | `KUDO_HUB_TEST_ADDR=host:port KUDO_HUB_TEST_NODE=name cargo test -p kudo-shim --test job_tools` |
| Agent CLI (worker loop) | hooks and worker tools tested as functions over a temp store | `kudo-shim worker <context>` in a pane |

The same assertions run on both sides, so a stand-in cannot quietly drift from
what it stands in for. Their first run against the deployed hub found a real
fault: the node's user service could not spawn any program.

Two limits of the hub's real side:

- **Same host.** The tests create workspaces and job directories under the
  test machine's temporary directory and the node resolves them on its own
  filesystem, so `KUDO_HUB_TEST_NODE` must name a node on the machine running
  the tests.
- **It leaves records.** Each run starts a handful of short jobs, and a node
  keeps every job's record (no eviction yet). Point it at the dev hub below,
  not at the hub your real sessions use.

A worker needs no hub: `kudo-shim worker <context>` with `KUDO_HUB_ADDR`
unset serves the worker tools on their own.

## The full topology

`moco-processes.styx` at the repository root declares a dev S3 (`dev-s3`), a
dev hub on `127.0.0.1:15401` (`dev-hub`) and a dev node dialing it
(`dev-node`: built and run inside this repository's dev shell, in its own
target directory `.dev/target`, state in `.dev/node`, job ports
20000-20999). Start them through the job tools of a node **on the machine that
holds this checkout** — the manifest and the jobs' working directory are the
checkout — e.g. `job_start dev-hub`, then run the shared suite against them:

```
KUDO_HUB_TEST_ADDR=127.0.0.1:15401 KUDO_HUB_TEST_NODE=dev \
  cargo test -p kudo-shim --test job_tools
```

`dev-s3` is `scripts/dev-s3`: the same port (9100) and data directory as running
the script by hand, so run one or the other, not both.
