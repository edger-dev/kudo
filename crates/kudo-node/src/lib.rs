//! kudo-node — the composition root that makes the job substrate reachable.
//!
//! This crate exists because offering moco's job substrate over the hub needs
//! something that knows **both**, and there was nowhere legal to put it:
//!
//! - putting the adapter in moco would bind the engine to the daemon's internal
//!   `Connector` trait, which the engine's own contract forbids;
//! - putting it in the daemon would make the transport depend on the engine,
//!   which the layering rule forbids.
//!
//! A composition root resolves that without weakening either rule: it may depend
//! on anything **because nothing depends on it**. It is a leaf of the dependency
//! graph, so binding a transport to an engine here creates no upward edge.
//!
//! Everything here is adapter. No policy, no state, no decisions — those belong
//! to the engine, and a composition root that starts making them has become an
//! undeclared component.
//!
//! implements: kudo:7ea24d8f08c2e3e9c4502030a86fd9d5f9e36c1067d7e2d034d2796177e719ec
//! implements: moco:3aa206d0a5ecd433ea159c78d3176934e98adef12bcca380b42ccf2b1ced591b

pub mod job;

pub use job::JobConnector;
