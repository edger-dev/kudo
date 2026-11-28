//! Intake-note presentation and implementation requests.

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ImplementationRequest {
    pub title: String,
    pub head: String,
    pub ask: String,
    pub node: String,
}

// implements: kudo::spec::console-intake@a4cebbc095b557f5c12d0b12913e5a561fc6d008a58020fd2c0ad7fb98f9aff3
pub fn implementation_request(
    note: &crate::model::IntakeNoteView,
    node: &str,
) -> ImplementationRequest {
    ImplementationRequest {
        title: note.title.clone(),
        head: format!(
            "# {}\nstate: active — new\nnext_action: decompose and execute in small serial steps\n\n## Intake note\nkino://{}\n\n{}\n",
            note.title, note.kino_id, note.body
        ),
        ask: format!(
            "Read design note kino://{}. Break the work into small serial steps. Execute the first ready step and submit evidence. Do not start parallel workers, worktrees, or pull requests.",
            note.kino_id
        ),
        node: node.to_owned(),
    }
}

pub fn apply_implementation(
    store: &kudo_context::ContextStore,
    note: &crate::model::IntakeNoteView,
    node: &str,
    at: i64,
) -> Result<String, String> {
    for card in store.contexts().map_err(|error| error.to_string())? {
        if store
            .head(&card.id)
            .map_err(|error| error.to_string())?
            .body
            .contains(&note.kino_id)
        {
            return Ok(card.id.0);
        }
    }
    let request = implementation_request(note, node);
    let context = store
        .create_context(&request.title, &request.head, at)
        .map_err(|error| error.to_string())?;
    store
        .declare_wake_node(&context, &request.node, at)
        .map_err(|error| error.to_string())?;
    store
        .enqueue(&context, &request.ask, at)
        .map_err(|error| error.to_string())?;
    Ok(context.0)
}
