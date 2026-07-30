# The three lenses

One job, three ways to look at it. They exist because a human and an agent want
genuinely different things from the same process, and serving both from one raw
byte stream serves neither.

| Lens | Tool | Answers |
|---|---|---|
| **Machine** | `job_read` | *What did it find?* |
| **Screen** | `job_screen` | *What does it look like right now?* |
| **Scrollback** | `job_tail` | *What exactly did it print?* |

## Machine — start here

A declared sidecar file plus a format name:

```styx
{name check, argv (cargo clippy --message-format json),
 machine_file ".diagnostics", machine_format "json"}
```

`job_read` returns those few hundred bytes instead of the whole terminal stream.
**This is what makes an agent read cheap**, and cheapness is not an optimisation
here — it decides whether the supervisor gets used at all.

It is **declared, never inferred**. Auto-structuring arbitrary output is a
research project; a declared sidecar is three lines of config and is always
right.

A job that declares no machine view falls back to scrollback, and the reply
**says which it gave you**. Handing back raw output labelled as structured would
be worse than handing back nothing, because a caller would try to parse it.

## Screen — for anything that redraws

A progress bar that rewrites one line with carriage returns is thousands of
superseded frames in scrollback and a single line on a screen. `job_screen`
returns the visible grid — not the history.

Declare `human_view @Terminal` and the job runs under a pty, so `isatty` is
true and it draws as it would for a person. That is not cosmetic: many tools
change what they emit the moment they detect a pipe.

The screen is folded **as the output goes past**, not replayed on demand — so it
stays correct even after the bytes that drew it have been discarded, and it
survives a daemon restart when the [PTY holder](./deployment.md) is installed.
The reply distinguishes `live` (observed) from `reconstructed` (replayed from
what scrollback still holds, and therefore blind to anything drawn before the
oldest retained byte).

A `@Terminal` job's pty has no *controlling* terminal: `isatty` is true and
redraw works, but job control does not, and a job reading stdin blocks as it
would at a terminal nobody is typing at.

## Scrollback — the exact bytes

`job_tail` returns output byte-exact, with the job's status attached to every
read — so polling `job_tail` is a complete way to observe a job, including
learning that it finished.

Pass the previous reply's `next_offset` to get only what is new. Offsets are
**logical**: they count bytes ever written, so they stay meaningful when old
output is discarded.

Scrollback is **bounded** (4 MiB by default). When a job out-runs it, the oldest
bytes go and the reply tells you how many you missed rather than quietly
handing you the wrong ones. A job noisy enough to hit that is precisely the job
that should be read through a declared machine view.

## Choosing

Reach for `job_read` first, `job_screen` when the thing redraws, and `job_tail`
when you need the literal bytes. If you find yourself tailing a build to find
out whether it passed, the job wants a `machine_file`.
