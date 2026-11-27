//! Shared rendering for structured session diagnostics.

// implements: hub::spec::session-diagnostics@0ed7d0e7b10814c751b8b3303cb9572a2f0b34b7c0047b49ac805c19d0e55033

use hub_protocol::DiagnosticReport;

pub fn render_report(report: &DiagnosticReport) -> String {
    let mut lines = vec![
        format!("status: {:?}", report.status).to_lowercase(),
        format!("source: {}", report.source),
        format!("target: {}", report.target),
        format!("probe: {}", report.probe_id),
        format!("observed_at_ms: {}", report.observed_at_ms),
        format!("elapsed_ms: {}", report.elapsed_ms),
    ];
    for check in &report.checks {
        lines
            .push(format!("{}: {:?} · {}", check.stage, check.status, check.detail).to_lowercase());
    }
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use hub_protocol::{DiagnosticCheck, DiagnosticStatus};

    #[test]
    fn report_contains_evidence_without_workspace_or_key_fields() {
        let report = DiagnosticReport {
            checks: vec![DiagnosticCheck {
                stage: "callback".into(),
                status: DiagnosticStatus::Ok,
                detail: "node yolo · harness kimi".into(),
            }],
            ..DiagnosticReport::new("p".into(), "hub".into(), "a".into())
        }
        .finish();
        let text = render_report(&report);
        assert!(text.contains("status: ok"));
        assert!(text.contains("callback: ok · node yolo · harness kimi"));
        assert!(!text.contains("workspace"));
        assert!(!text.contains("private_key"));
        assert!(!text.contains("HUB_AUTH_TOKEN"));
    }
}
