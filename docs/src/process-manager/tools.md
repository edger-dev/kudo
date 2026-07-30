# Agent tools

The MCP shim exposes the supervisor to an agent. It runs **one per workspace**,
launched in the project directory — that directory is how the engine knows which
workspace a write belongs to, so nothing has to be configured.

Set `KUDO_HUB_ADDR` to the hub's address. Every node is reached through the hub;
there is no local listener to fall back on.

## Reading

| Tool | Use it for |
|---|---|
| `nodes` | What machines and connectors exist. |
| `job_list` | Every job on a node, with owner, status and port. |
| `job_read` | A job's findings, through its machine view. **Prefer this.** |
| `job_screen` | What a redrawing job looks like right now. |
| `job_tail` | Exact output; pass `next_offset` to resume. |
| `job_stats` | CPU and memory, now and at peak. |

## Writing

| Tool | Use it for |
|---|---|
| `job_start` | Start a declared job by name. |
| `job_start_adhoc` | Run something not in the manifest. |
| `job_ensure` | Bring up `@Session` jobs; safe to re-run. |
| `job_restart` | Re-read the declaration and start again. |
| `job_kill` | Stop a job. Its record and output stay readable. |
| `job_clear` | Remove finished entries. Never signals anything. |

Every tool takes `node`, plus optional `link` and `connector`.

## Things worth knowing

**Reads are global, writes are not.** You can list and read every job on the
machine; you may only stop or restart jobs your workspace owns. A refusal names
the owner.

**A timeout is not a failure.** If a call exceeds its budget the reply says the
work may still be running. Treat it as unknown, not as "nothing happened" —
otherwise you start the same work twice.

**`job_kill` is a stop, not a crash.** A stopped job is not restarted, whatever
its restart policy says: an explicit stop must not be fought by the supervisor.
A job that *crashes* is restarted according to its policy.

**Resource limits are advisory.** `job_stats` reports a breach and nothing acts
on it — not the supervisor, and not you. A job over its ceiling is
overwhelmingly the job someone is diagnosing.

**`job_clear` removes tombstones only.** A crashed job lingers so you can see
*that* it died and read its last output. Clearing never signals a live job, and
never moves a declaration's port.
