# Spec — a worker's launch pre-answers the agent CLI's first-run prompts

**Contract.** The launch plan that starts a worker's agent CLI passes the
options and settings that keep it from stopping at an interactive prompt no
one will answer: project-scoped MCP servers found in the repository are not
offered, and promotions and first-run tips are switched off. Where the node asks
for a strict MCP config (kino://130ca83438842f6cd1eeda1eafdda4a1cd34e9c5e9530587ea39bb85b02a06b9), strict mode declines them;
elsewhere the launch names the servers in every `.mcp.json` the CLI reads (the
repository's directory and each one above it) as declined in the settings it
passes, so the CLI loads the user's own servers and
asks about none of the repository's. The prompts kudo already answers by design
(the folder trust, through the node's own settings) stay as they are.

**Chosen.** An unattended pane has no person at it; a prompt there means a
silent worker until the waker's back-off passes. Pre-answering what is known
is cheaper than detecting and reporting a stuck pane.

**Rejected.** Detecting a pane stuck at a prompt (no trace after N seconds) and
reporting it for a person to answer: it fixes nothing unattended. (It may
still be added for prompts not yet known.)

**Rejected (23 Oct).** Strict mode on every launch, to decline project servers:
it also dropped the user's own servers, which the second deployment's checks
need.

**Escape hatch.** A person taking over the pane answers anything new.
