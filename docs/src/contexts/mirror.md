# The context store mirror

A context store lives on each machine (`~/.local/state/kudo/contexts`, or
`KUDO_CONTEXT_STORE`). Give it a **mirror** — an S3-compatible bucket — and
every machine pointed at the same mirror sees one store: an item enqueued on
one is ready on the other, a claim taken on one is seen before the other
claims, and a submit on a head the other machine moved is refused.

The mirror is configured from the environment:

| Variable | Meaning |
|---|---|
| `KUDO_CONTEXT_MIRROR` | `s3://bucket/prefix` (or `file:///dir`) |
| `AWS_ENDPOINT` | the S3 endpoint; `http://…` allows plain HTTP (local use) |
| `AWS_REGION` | `auto` for Cloudflare R2, `us-east-1` for MinIO |
| `AWS_ACCESS_KEY_ID`, `AWS_SECRET_ACCESS_KEY` | the key |

Every `kudo-shim` command, hook and worker tool reads them. Without
`KUDO_CONTEXT_MIRROR` the store is local only, as before.

## How it syncs

Each operation pulls the mirror before it reads (plain reads at most once a
second); claims, submits, releases and repo declarations always pull first.
Each write is pushed as it lands, and only new files are sent. Nothing is ever
overwritten in the mirror: every file is named by its hash.

- **Offline**: the machine works on its local store and retries the mirror
  after 30 seconds; its writes go up on the first sync that reaches it.
- **Refused** (wrong key, no permission): the command fails with
  "the context store mirror refused this machine" — fix the configuration;
  kudo never silently falls back to local work.

## A development S3

For development, run MinIO by hand in the container:

```
scripts/dev-s3                                   # foreground; Ctrl-C stops it
source ~/.local/state/kudo/dev-s3/mirror.env     # in any other pane
kudo-shim context list
```

It listens on `127.0.0.1:9100` with a fixed development key and keeps its data
between runs. Cloudflare R2 later is the same variables with R2's endpoint
(`https://<account-id>.r2.cloudflarestorage.com`), `AWS_REGION=auto` and an R2
API token's keys.
