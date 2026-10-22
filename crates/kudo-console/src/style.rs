//! The console's stylesheet: colour tokens for light and dark, the dense
//! monospace visual language, and the phone's one-pane drill.
//!
//! implements: 2dd989d6ce0f4135c0d8d3ab6e99e065ed03fe3dbf1827d5621d289458cd4351@2dd989d6ce0f4135c0d8d3ab6e99e065ed03fe3dbf1827d5621d289458cd4351
//! implements: 77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90@77d223e031a8cefaf954bd0d4708fc301867286d9d92c547811d5335d7caeb90

/// The stylesheet the shell carries. No component names a colour: each
/// reads a token, and a theme is a set of tokens.
pub const STYLE: &str = r#"
html, body { margin: 0; padding: 0; }
.kudo-shell {
  --bg: #fafafa; --fg: #1d1d1f; --muted: #6b6b70; --line: #d9d9de;
  --panel: #ffffff; --hover: #f0f0f3; --accent: #2a5bd7;
  --ok: #17803d; --warn: #a76200; --bad: #c4231c; --idle: #77777c;
  --warn-bg: #fff4e0; --bad-bg: #fde8e7; --ok-bg: #e5f5ea;
  --term-bg: #111114; --term-fg: #e2e2e6;
  color-scheme: light;
}
.kudo-shell[data-theme="dark"] {
  --bg: #141416; --fg: #e4e4e7; --muted: #9a9aa2; --line: #2e2e33;
  --panel: #1b1b1e; --hover: #26262b; --accent: #7aa2ff;
  --ok: #4ec27a; --warn: #f0a838; --bad: #ff6b62; --idle: #8a8a92;
  --warn-bg: #3a2a10; --bad-bg: #3d1614; --ok-bg: #12301d;
  --term-bg: #0b0b0d; --term-fg: #e2e2e6;
  color-scheme: dark;
}
@media (prefers-color-scheme: dark) {
  .kudo-shell:not([data-theme="light"]) {
    --bg: #141416; --fg: #e4e4e7; --muted: #9a9aa2; --line: #2e2e33;
    --panel: #1b1b1e; --hover: #26262b; --accent: #7aa2ff;
    --ok: #4ec27a; --warn: #f0a838; --bad: #ff6b62; --idle: #8a8a92;
    --warn-bg: #3a2a10; --bad-bg: #3d1614; --ok-bg: #12301d;
    --term-bg: #0b0b0d; --term-fg: #e2e2e6;
    color-scheme: dark;
  }
}
.kudo-shell {
  position: fixed; inset: 0; display: flex; flex-direction: column;
  background: var(--bg); color: var(--fg);
  font: 13px/1.45 ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-variant-numeric: tabular-nums;
}
.kudo-shell * { box-sizing: border-box; }
.kudo-shell button {
  font: inherit; color: var(--fg); background: var(--panel);
  border: 1px solid var(--line); border-radius: 6px; padding: 4px 10px;
  min-height: 32px; cursor: pointer;
}
.kudo-shell button:hover { background: var(--hover); }
.kudo-shell button:disabled { opacity: .5; cursor: default; }
.kudo-shell button.bad { color: var(--bad); border-color: var(--bad); }
.kudo-shell button.armed { background: var(--bad-bg); color: var(--bad); border-color: var(--bad); white-space: normal; text-align: left; }
.kudo-shell input {
  font: inherit; color: var(--fg); background: var(--panel);
  border: 1px solid var(--line); border-radius: 6px; padding: 6px 8px;
  min-height: 34px; min-width: 0; flex: 1;
}
.kudo-shell pre { margin: 0; white-space: pre-wrap; overflow-wrap: anywhere; }
.kudo-shell h3, .kudo-shell .label {
  margin: 10px 0 4px; font-size: 11px; font-weight: 600; letter-spacing: .08em;
  text-transform: uppercase; color: var(--muted);
}
.kudo-shell .muted { color: var(--muted); }
.kudo-shell .row { display: flex; gap: 6px; align-items: center; flex-wrap: wrap; }
.kudo-shell form.row { flex-wrap: nowrap; margin: 6px 0; }

.kudo-top {
  display: flex; align-items: center; gap: 8px; padding: 6px 10px;
  border-bottom: 1px solid var(--line); background: var(--panel); flex: none;
}
.kudo-top .crumb { flex: 1; min-width: 0; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.kudo-band { padding: 6px 10px; background: var(--bad-bg); color: var(--bad); flex: none; }
.kudo-band button { margin-left: 8px; min-height: 24px; padding: 0 8px; }

.badge { border-radius: 999px; padding: 2px 10px; white-space: nowrap; }
.badge[data-badge="none"] { color: var(--muted); }
.badge[data-badge="waiting"] { background: var(--warn-bg); color: var(--warn); border-color: var(--warn); font-weight: 600; }
.badge[data-badge="unknown"] { background: var(--bad-bg); color: var(--bad); border-color: var(--bad); }

.kudo-panes { flex: 1; display: flex; min-height: 0; }
.kudo-panes[data-direction="column"] { flex-direction: column; }
.kudo-panes > [data-pane] {
  overflow: auto; padding: 8px 10px; min-width: 0; min-height: 0;
  border-right: 1px solid var(--line);
}
.kudo-panes > [data-pane="where"] { max-width: 360px; background: var(--panel); }

.item { padding: 6px 8px; border-radius: 6px; cursor: pointer; border: 1px solid transparent; }
.item:hover { background: var(--hover); }
.item[data-selected="true"] { border-color: var(--accent); }
.item .title { font-weight: 600; }
.workers { padding-left: 12px; }

[data-tone="ok"] .state, .state[data-tone="ok"] { color: var(--ok); }
[data-tone="warn"] .state, .state[data-tone="warn"] { color: var(--warn); }
[data-tone="bad"] .state, .state[data-tone="bad"] { color: var(--bad); }
[data-tone="idle"] .state, .state[data-tone="idle"] { color: var(--idle); }
.job { border-left: 3px solid var(--line); }
.job[data-tone="ok"] { border-left-color: var(--ok); }
.job[data-tone="warn"] { border-left-color: var(--warn); }
.job[data-tone="bad"] { border-left-color: var(--bad); }

.card { border: 1px solid var(--warn); border-left-width: 4px; border-radius: 6px; padding: 8px; margin: 8px 0; background: var(--panel); }
.card .argv { font-weight: 600; overflow-wrap: anywhere; }

.term { background: var(--term-bg); color: var(--term-fg); padding: 8px; border-radius: 6px; white-space: pre; overflow-x: auto; }
.follow {
  display: flex; flex-direction: column-reverse; overflow: auto;
  max-height: calc(100dvh - 180px); background: var(--term-bg); color: var(--term-fg);
  padding: 8px; border-radius: 6px;
}
.trace-row { padding: 2px 0; border-bottom: 1px solid var(--line); overflow-wrap: anywhere; }

.tray-dim { position: fixed; inset: 0; background: rgba(0,0,0,.35); z-index: 10; }
.tray {
  position: fixed; top: 0; right: 0; bottom: 0; width: min(520px, 100vw);
  background: var(--bg); border-left: 1px solid var(--line); z-index: 11;
  overflow: auto; padding: 10px 12px; outline: none;
}
.tray .unknown { color: var(--bad); }
.tray .outcome { color: var(--ok); margin: 6px 0; }
.tray .outcome.failed { color: var(--bad); }
.result { margin: 4px 0; }
.result.failed { color: var(--bad); }

.tabs { display: flex; gap: 6px; margin-bottom: 6px; }
.tabs button[data-on="true"] { border-color: var(--accent); color: var(--accent); }

@media (min-width: 721px) {
  .narrow-only { display: none !important; }
}
@media (max-width: 720px) {
  .kudo-shell[data-narrow="drill"] .kudo-panes { flex-direction: column; }
  .kudo-shell[data-narrow="drill"] .kudo-panes > [data-pane]:not([data-focus]) { display: none; }
  .kudo-shell[data-narrow="drill"] .kudo-panes > [data-pane] { flex: 1 !important; max-width: none; border: none; }
  .kudo-shell button { min-height: 40px; }
}
"#;
