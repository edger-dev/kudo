//! The per-job PTY holder, shipped as part of the node.
//!
//! The mechanism is the engine's — this is fifteen lines of `main` around
//! `moco_job::holder`. It lives here because **a binary of a dependency is not
//! built by cargo**: the engine declares the same bin target for its own tests,
//! but nothing about depending on that crate produces the executable. Shipping
//! it is a deployment concern, and deployment concerns belong to the
//! composition root.
//!
//! The name matters: `kudo-node` looks for `moco-pty-holder` beside itself.
//!
//! implements: pty-holder-owns-the-terminal

fn main() -> std::process::ExitCode {
    let config = match moco_job::holder::config_from_args(std::env::args().skip(1)) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("moco-pty-holder: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    match moco_job::holder::run(config) {
        // Exit with the *job's* code, so the daemon reads the job's outcome
        // rather than the holder's.
        Ok(code) => std::process::ExitCode::from(code.clamp(0, 255) as u8),
        Err(e) => {
            eprintln!("moco-pty-holder: {e}");
            std::process::ExitCode::from(1)
        }
    }
}
