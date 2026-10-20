//! kudo-context — the context engine.
//!
//! A **context** is the unit of memory a worker is bound to: a superseded head
//! (where / next / established / open), its log of submissions, the repos it
//! spans, and its queue of items
//! (`kudo:6f2cfc2264e8f880d1300d1d79b96c597b995b36b756a03d6871a945b986f2dd`).
//! Its store must be reachable with no node online
//! (`kudo:4909f3e9f6d391d8a20dc8587a1ffc33160eb238d86d1cc55d5b9fb9ce754e76`).
//!
//! This crate is an engine, not a service: every composition root that needs
//! contexts links it and talks to the store directly. Empty until phase 1
//! gives it pull and submit.
//!
//! task: 4aef99814b668f7cbb8900eee43e35b67fdb6dbcd18ba196858f6226874cc0e2@715de6400c53b648fca5c4b6005b5b0be2327af9d505061c29fd17377f6f565a
