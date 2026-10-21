//! The shim's local commands: create and drive a context, and start a worker
//! in this terminal. A thin face over the context engine — every answer is the
//! engine's.

use std::path::{Path, PathBuf};

use kudo_context::{ContextId, ContextStore};

const USAGE: &str = "usage: kudo-shim context new <title>\n       \
                     kudo-shim context enqueue <context> <ask>\n       \
                     kudo-shim context show <context>\n       \
                     kudo-shim context list\n       \
                     kudo-shim worker <context>";

/// Where the context store lives: `KUDO_CONTEXT_STORE`, else
/// `$XDG_STATE_HOME/kudo/contexts`, else `~/.local/state/kudo/contexts`.
pub fn default_store_dir() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var(kudo_worker::env::STORE_ENV) {
        return Some(PathBuf::from(dir));
    }
    let state = std::env::var("XDG_STATE_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| Path::new(&h).join(".local/state"))
        })?;
    Some(state.join("kudo/contexts"))
}

/// Run `kudo-shim context <args…>` against the store in `store_dir`; the text
/// to print, or an error.
pub fn context(store_dir: &Path, args: &[String], now: i64) -> Result<String, String> {
    let words: Vec<&str> = args.iter().map(String::as_str).collect();
    match words.as_slice() {
        ["new", title @ ..] if !title.is_empty() => {
            let title = title.join(" ");
            let store = open_or_init(store_dir)?;
            let head = format!(
                "# {title}\nstate:       active — new\nwhere:       nothing done yet\n\
                 next_action: pull the first item · who: worker\n"
            );
            let id = store
                .create_context(&title, &head, now)
                .map_err(|e| e.to_string())?;
            Ok(format!("{}\n", id.0))
        }
        ["enqueue", ctx, ask @ ..] if !ask.is_empty() => {
            let store = open(store_dir)?;
            let ctx = named(&store, ctx)?;
            let item = store
                .enqueue(&ctx, &ask.join(" "), now)
                .map_err(|e| e.to_string())?;
            Ok(format!("enqueued {}\n", item.0))
        }
        ["show", ctx] => {
            let store = open(store_dir)?;
            let ctx = named(&store, ctx)?;
            let head = store.head(&ctx).map_err(|e| e.to_string())?;
            let ready = store.ready_count(&ctx, now).map_err(|e| e.to_string())?;
            let inbox = store.inbox(&ctx).map_err(|e| e.to_string())?;
            let log = store.log(&ctx).map_err(|e| e.to_string())?;
            let mut out = format!(
                "{}
-- {ready} ready, {} submitted, {} in the inbox
",
                head.body.trim_end(),
                log.len(),
                inbox.len()
            );
            for (n, entry) in log.iter().enumerate() {
                out.push_str(&format!("\n[{}] {}\n", n + 1, entry.trim_end()));
            }
            for entry in &inbox {
                out.push_str(&format!("\ninbox: {} ({})\n", entry.line, entry.lease));
            }
            Ok(out)
        }
        ["list"] => {
            if !store_dir.join(".kinora").is_dir() {
                return Ok(NO_CONTEXTS.to_owned());
            }
            let store = open(store_dir)?;
            let all = store.contexts().map_err(|e| e.to_string())?;
            if all.is_empty() {
                return Ok(NO_CONTEXTS.to_owned());
            }
            let mut out = String::new();
            for c in &all {
                let ready = store.ready_count(&c.id, now).map_err(|e| e.to_string())?;
                let submitted = store.log(&c.id).map_err(|e| e.to_string())?.len();
                let short = c.id.0.get(..SHORT_ID).unwrap_or(&c.id.0);
                out.push_str(&format!(
                    "{short}  {}  ({ready} ready, {submitted} submitted, created {})\n",
                    c.title, c.created
                ));
            }
            Ok(out)
        }
        _ => Err(USAGE.to_owned()),
    }
}

const NO_CONTEXTS: &str = "no contexts yet: kudo-shim context new <title>\n";

/// How much of a context id `list` shows; any unique prefix is accepted back.
const SHORT_ID: usize = 12;

/// The context a full id or a unique prefix names.
fn named(store: &ContextStore, id_or_prefix: &str) -> Result<ContextId, String> {
    store
        .resolve_context(id_or_prefix)
        .map_err(|e| e.to_string())
}

fn open(dir: &Path) -> Result<ContextStore, String> {
    ContextStore::open(dir).map_err(|e| e.to_string())
}

fn open_or_init(dir: &Path) -> Result<ContextStore, String> {
    if dir.join(".kinora").is_dir() {
        return open(dir);
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    ContextStore::init(dir).map_err(|e| e.to_string())
}

/// Replace this process with the agent CLI, started as a worker under `ctx`.
/// Returns only on failure.
pub fn exec_worker(store_dir: &Path, ctx: &str) -> String {
    use std::os::unix::process::CommandExt;

    let Ok(hub) = std::env::var(crate::HUB_ADDR_ENV) else {
        return format!(
            "{} is not set: the worker's MCP server dials the hub",
            crate::HUB_ADDR_ENV
        );
    };
    let ctx = match open(store_dir).and_then(|s| named(&s, ctx)) {
        Ok(ctx) => ctx,
        Err(e) => return e,
    };
    let shim = match std::env::current_exe() {
        Ok(path) => path,
        Err(e) => return format!("cannot find this binary: {e}"),
    };
    let lease = match kudo_worker::new_lease_id() {
        Ok(lease) => lease,
        Err(e) => return format!("cannot make a lease id: {e}"),
    };
    let setup = kudo_worker::WorkerSetup {
        store_dir: store_dir.to_path_buf(),
        context: ctx.clone(),
        lease,
        claim_ttl: env_num(kudo_worker::env::CLAIM_TTL_ENV)
            .unwrap_or(kudo_worker::DEFAULT_CLAIM_TTL),
        max_turns: env_num(kudo_worker::env::MAX_TURNS_ENV)
            .unwrap_or(i64::from(kudo_worker::DEFAULT_MAX_TURNS))
            .try_into()
            .unwrap_or(kudo_worker::DEFAULT_MAX_TURNS),
    };
    let cli = std::env::var("KUDO_AGENT_CLI").unwrap_or_else(|_| "claude".to_owned());
    let plan = kudo_worker::plan(&setup, &cli, &shim, &hub);
    let Some((program, rest)) = plan.argv.split_first() else {
        return "empty launch plan".to_owned();
    };
    eprintln!("kudo: worker lease {} under context {}", setup.lease, ctx.0);
    let err = std::process::Command::new(program)
        .args(rest)
        .envs(plan.env)
        .exec();
    format!("cannot start {program}: {err}")
}

fn env_num(key: &str) -> Option<i64> {
    std::env::var(key).ok()?.parse().ok()
}
