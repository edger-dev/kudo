//! kudo-worker — the agent CLI as a pull worker.
//!
//! A worker is the ordinary interactive CLI in a terminal pane, started under a
//! context. The Stop hook drives its pull loop, and a lease is one context
//! that ends on any of four conditions
//! (`kudo:b9ef8909cadf0092e9b9d01eb3fc3edbe9db4f2af17f80753571d9434628b1af`). A
//! session is a lease, not the unit of memory
//! (`kudo:2c5f95fe8abf6cae1a31735dfd66d55816132632dbce7438e2c1f7038dafccc6`).
//!
//! This crate holds the hook decisions — what SessionStart injects, whether
//! Stop hands over the next item or lets the session end — as plain functions
//! over the context store and a small per-lease record. Turning hook JSON into
//! calls here is the shim's job.
//!
//! task: 4aef99814b668f7cbb8900eee43e35b67fdb6dbcd18ba196858f6226874cc0e2@715de6400c53b648fca5c4b6005b5b0be2327af9d505061c29fd17377f6f565a

pub mod env;
pub mod launch;
pub mod lease;
pub mod pane;
pub mod worker;

pub use env::{DEFAULT_CLAIM_TTL, DEFAULT_MAX_TURNS, KICKOFF, WorkerSetup};
pub use launch::{LaunchPlan, new_lease_id, plan};
pub use lease::LeaseRecord;
pub use lease::{LeaseBook, LeaseEnd};
pub use worker::{Observer, Pull, Stop, Worker, WorkerConfig, render_heads};
