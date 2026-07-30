# The manifest

Declared jobs live in **`moco-processes.styx`** at the root of a workspace. It
is read fresh on every start, so editing it takes effect without restarting
anything.

```styx
proc (
  {name web,
   argv (sh -c "cargo run --bin server -- --port @MOCO_PORT"),
   port @Auto, lifetime @Service, restart @Always, autostart @Session},

  {name check,
   argv (cargo clippy --workspace --message-format json),
   machine_file ".diagnostics", machine_format "json"},

  {name dash,
   argv (btop),
   human_view @Terminal}
)
```

## Fields

Only `name` and `argv` are required.

| Field | Meaning | Default |
|---|---|---|
| `name` | Unique within the workspace. The qualified id is `workspace:name`, so a second checkout may declare its own `check`. | — |
| `argv` | What to run, as an argument vector. | — |
| `cwd` | Where to run it, relative to the workspace root. | the root |
| `deadline_ms` | Execution deadline; `0` is unbounded. | `0` |
| `lifetime` | `@OneShot` or `@Service`. | `@OneShot` |
| `restart` | `@Never`, `@OnFailure`, `@Always`. Meaningful only for a service. | `@Never` |
| `autostart` | `@Manual`, `@Session`, `@Boot`. | `@Manual` |
| `port` | `@None`, `@Auto`, or `@Fixed{port 8080}`. | `@None` |
| `port_env` | Variable the port arrives in. | `MOCO_PORT` |
| `worktree` | `@Each` or `@MainOnly`. Unstated resolves from the port. | see below |
| `hosts` | Node names allowed to run it. Empty means anywhere. | anywhere |
| `human_view` | `@Logs` or `@Terminal`. | `@Logs` |
| `machine_file` | Sidecar holding a machine-readable view, relative to the job's cwd. | none |
| `machine_format` | What is in that file — a label, never a parser. | none |
| `cpu_pct` | Advisory CPU ceiling, percent of one core. `400` is four cores. | unset |
| `mem_mb` | Advisory memory ceiling, MiB. | unset |

## argv is a vector, never a shell string

Shell metacharacters stay inert data, and a rule-set matching exact argv has
something it can soundly match. Where you genuinely need a shell — pipes,
redirection, `$VAR` — say so explicitly with `sh -c`, and it is the shell that
is being declared:

```styx
argv (sh -c "cargo build 2>&1 | tee build.log")
```

## Ports: `@MOCO_PORT`, not `$PORT`

Declare `port @Auto` and the node allocates one, delivered to the child in
`$MOCO_PORT` (or whatever `port_env` names). A program that wants it on the
command line uses the **`@MOCO_PORT`** token — **quoted**, because `@` is Styx's
own sigil for an enum variant and a bare `@MOCO_PORT` will not parse:

```styx
argv (myserver --port "@MOCO_PORT")
argv (myserver "--port=@MOCO_PORT")     # attached form works too
```

The sigil is `@`, not `$`, because the sigil names *who expands it*. `$` means a
shell did, and there is no shell. Substitution happens after the rule-set has
matched, cannot change the argument count, and only `@MOCO_*` names are
node-supplied — so no other `@` needs escaping.

Auto ports are **sticky**: a declaration gets the same port back across
restarts, daemon restarts and `job_clear`, as long as it is still free. Default
range is `10000-19999`, overridable with `MOCO_PORT_RANGE=low-high`.

A **fixed** port implies `worktree @MainOnly`, because two worktrees cannot bind
one port. State `worktree @Each` explicitly if you really want every worktree to
try, or use `@Auto` so each gets its own.

## Autostart

- **`@Session`** — brought up by an agent or tool at session start (`job_ensure`).
- **`@Boot`** — started by the node daemon at startup. Only valid in the **node**
  manifest (see [Deployment](./deployment.md)); a workspace declaring it is
  refused, because nothing discovers an arbitrary workspace at boot and the
  entry would silently never run.
- **`@Manual`** — waits to be asked.

## Syntax notes

The format is [Styx](https://github.com/edger-dev/styx). Two things reliably
catch people out:

**Enum values take `@`**: `@Terminal`, `@Auto`, `@OnFailure`. A bare word is a
string.

**Quote anything that looks like a value.** Styx reads bare words structurally,
so three things in an argv need quotes:

```styx
argv ("/bin/true")                      # `true` is a boolean
argv (myserver --port "@MOCO_PORT")     # `@` starts a variant
argv (sh -c "cargo build 2>&1 | tee x") # spaces and metacharacters
```

When in doubt, quote it. A quoted string is always just a string.

A malformed manifest is an **error**, not an empty one: one bad field does not
silently void every declaration in the file. Two entries with the same name are
refused rather than last-one-wins.
