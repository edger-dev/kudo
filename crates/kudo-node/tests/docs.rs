//! The documented manifest examples actually parse.
//!
//! Written after two of them did not. A manifest example is a promise that a
//! reader will paste into a file, so an example that does not parse is worse
//! than no example — it costs the reader a debugging session before they doubt
//! the documentation. Both errors here were the same kind: Styx reads bare
//! words structurally, and a bare `@MOCO_PORT` is a variant tag rather than the
//! port token.
//!
//! implements: boot-autostart-reads-the-node-manifest

use std::sync::atomic::{AtomicU64, Ordering};

use moco_job::{MANIFEST_FILE, Manifest};

static SEQ: AtomicU64 = AtomicU64::new(0);

const PAGES: &[(&str, &str)] = &[
    (
        "manifest.md",
        include_str!("../../../docs/src/process-manager/manifest.md"),
    ),
    (
        "overview.md",
        include_str!("../../../docs/src/process-manager/overview.md"),
    ),
    (
        "lenses.md",
        include_str!("../../../docs/src/process-manager/lenses.md"),
    ),
    (
        "deployment.md",
        include_str!("../../../docs/src/process-manager/deployment.md"),
    ),
];

/// Every ```styx block in a page.
fn styx_blocks(page: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut current: Option<String> = None;
    for line in page.lines() {
        match (&mut current, line.trim_start()) {
            (None, l) if l.starts_with("```styx") => current = Some(String::new()),
            (Some(_), l) if l.starts_with("```") => {
                if let Some(block) = current.take() {
                    blocks.push(block);
                }
            }
            (Some(buf), _) => {
                buf.push_str(line);
                buf.push('\n');
            }
            _ => {}
        }
    }
    blocks
}

/// Strip a trailing `# ...` comment, which the docs use to annotate examples.
fn uncomment(line: &str) -> &str {
    line.split_once('#')
        .map_or(line, |(code, _)| code)
        .trim_end()
}

#[allow(
    clippy::expect_used,
    reason = "test helper: a failure is a broken harness"
)]
fn parses(text: &str) -> Result<usize, String> {
    let dir = std::env::temp_dir().join(format!(
        "kudo-docs-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create");
    std::fs::write(dir.join(MANIFEST_FILE), text).expect("write");

    // The node loader, because it accepts everything the workspace loader does
    // plus `autostart @Boot` — and the docs legitimately show both.
    let result = Manifest::load_node(&dir)
        .map(|m| m.proc.len())
        .map_err(|e| e.to_string());
    let _ = std::fs::remove_dir_all(&dir);
    result
}

#[test]
fn every_documented_manifest_example_parses() {
    let mut checked = 0;
    for (page, body) in PAGES {
        for block in styx_blocks(body) {
            if block.trim_start().starts_with("proc") {
                let n = parses(&block).unwrap_or_else(|e| {
                    panic!("{page}: a documented manifest does not parse.\n{e}\n---\n{block}")
                });
                assert!(n > 0, "{page}: a documented manifest declares nothing");
                checked += 1;
                continue;
            }

            // A single entry shown on its own, possibly across several lines.
            if block.trim_start().starts_with('{') {
                let wrapped = format!("proc ({})", block.trim());
                parses(&wrapped).unwrap_or_else(|e| {
                    panic!("{page}: a documented entry does not parse.\n{e}\n---\n{block}")
                });
                checked += 1;
                continue;
            }

            // A fragment: each line is one field, shown on its own. Wrap it in
            // the smallest manifest that can carry it.
            for line in block
                .lines()
                .map(uncomment)
                .filter(|l| !l.trim().is_empty())
            {
                let wrapped = format!("proc ({{name probe, argv (x), {line}}})");
                parses(&wrapped).unwrap_or_else(|e| {
                    panic!("{page}: a documented fragment does not parse.\n{e}\n---\n{line}")
                });
                checked += 1;
            }
        }
    }

    assert!(
        checked >= 8,
        "only {checked} examples were checked — the extractor has probably \
         stopped finding them, which would make this test pass by doing nothing"
    );
}
