# The Process Manager

A machine-global supervisor for the processes around your work: dev servers,
watchers, builds, checkers. It exists to answer three questions that nothing
else was answering.

**Where is that server running?** Long-lived processes get started inside a
terminal or an agent session, and then belong to it. Close the session and the
server dies; keep it and nobody else can see the process, read its output, or
say which checkout it came from.

**What is wedging this machine?** A runaway build can freeze a box while nothing
has a view of the process set wide enough to name the culprit.

**Did it compile?** A build's output is tens of thousands of tokens of ANSI and
redraw. An agent that must read all of it to answer one question learns to stop
asking.

## The model

A **node** is one machine. It runs one supervisor daemon, which owns every job
on that machine regardless of who asked for it.

A **workspace** is a repository checkout — a directory with a `.git` entry,
found by walking up from wherever a caller happens to be. A workspace *owns* the
jobs it declares.

A **job** is one supervised process. It has a stable id, a durable record on
disk, and an owner.

Reads are node-global; **writes are scoped to the owning workspace**. Any
session can list and read every job on the machine — that is the whole point of
a machine-global view — but only the workspace that owns a job may stop or
restart it. One checkout cannot kill another's server by accident.

## Jobs outlive whoever started them

This is the property everything else rests on. A job survives the session that
asked for it, and it survives the supervisor itself: the daemon can be stopped,
upgraded and restarted while its jobs keep running, and it takes them back over
when it returns. A job's pid and the kernel's start time for that pid are on
disk, so the returning daemon recognises the *same* process rather than a
recycled pid.

Consequently the daemon's own service unit must not kill its jobs when it stops
— see [Deployment](./deployment.md), where that is one line and load-bearing.

## Declared, not remembered

Jobs are **declared** in a manifest checked into the repository, so what should
be running is a fact about the project rather than about your shell history. The
manifest declares; the node authorizes. A declaration is a request to run
something, not permission — the node's rule-set still decides, and a job may be
refused for reasons the manifest cannot see, such as running on the wrong
machine or the wrong worktree.

Ad-hoc jobs are still possible for one-off work, and are marked as such.

Read on: [the manifest](./manifest.md), [the three lenses](./lenses.md), the
[agent tools](./tools.md), and [deployment](./deployment.md).
