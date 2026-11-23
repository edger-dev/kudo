use kudo_intake::{AdmissionError, AdmissionRules, Note};

fn note() -> Note {
    Note {
        target: "kudo".into(),
        title: "A design thought".into(),
        body: "The receiver should stay thin.\n\nInline `send_note` is fine.".into(),
        attestation: true,
        author: "YJ Park".into(),
        machine: "work".into(),
        session: "session-123".into(),
        created_at: "2026-11-23T12:00:00Z".into(),
    }
}

fn rules() -> AdmissionRules {
    AdmissionRules {
        max_body_bytes: 1024,
        sensitive_markers: vec![],
    }
}

#[test]
fn a_reviewed_text_note_is_accepted() {
    assert_eq!(note().validate(&rules()), Ok(()));
}

#[test]
fn attestation_is_required() {
    let mut note = note();
    note.attestation = false;
    assert_eq!(
        note.validate(&rules()),
        Err(AdmissionError::MissingAttestation)
    );
}

#[test]
fn every_required_field_must_be_nonblank() {
    for (field, value) in [
        ("target", ""),
        ("title", "  "),
        ("body", "\n\t"),
        ("author", ""),
        ("machine", ""),
        ("session", ""),
        ("created_at", ""),
    ] {
        let mut note = note();
        match field {
            "target" => note.target = value.into(),
            "title" => note.title = value.into(),
            "body" => note.body = value.into(),
            "author" => note.author = value.into(),
            "machine" => note.machine = value.into(),
            "session" => note.session = value.into(),
            "created_at" => note.created_at = value.into(),
            _ => unreachable!(),
        }
        assert_eq!(
            note.validate(&rules()),
            Err(AdmissionError::BlankField(field)),
            "{field}"
        );
    }
}

#[test]
fn the_limit_counts_utf8_bytes() {
    let mut note = note();
    note.body = "ééé".into();
    assert_eq!(
        note.validate(&AdmissionRules {
            max_body_bytes: 5,
            sensitive_markers: vec![],
        }),
        Err(AdmissionError::BodyTooLarge { actual: 6, max: 5 })
    );
}

#[test]
fn fenced_and_indented_code_blocks_are_refused() {
    for body in [
        "```rust\nfn main() {}\n```",
        "A thought follows.\n\n    fn main() {}",
        "\tlet x = 1;",
    ] {
        let mut note = note();
        note.body = body.into();
        assert_eq!(note.validate(&rules()), Err(AdmissionError::CodeBlock));
    }
}

#[test]
fn sensitive_markers_match_case_insensitively() {
    let rules = AdmissionRules {
        sensitive_markers: vec!["employername".into(), "project-x".into()],
        ..rules()
    };
    let mut note = note();
    note.body = "This concerns EmployerName.".into();
    assert_eq!(
        note.validate(&rules),
        Err(AdmissionError::SensitiveMarker("employername".into()))
    );
    note.body = "The receiver should stay thin.".into();
    note.title = "EmployerName planning".into();
    assert_eq!(
        note.validate(&rules),
        Err(AdmissionError::SensitiveMarker("employername".into()))
    );
    note.title = "A design thought".into();
    assert_eq!(note.validate(&rules), Ok(()));
}
