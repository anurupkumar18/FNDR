//! Hand-off (ADR 032): a person-approved job where a sandboxed Codex worker
//! turns a task plus attached memories and files into finished files in one
//! job folder, checked by FNDR before anything is shown as done.
//!
//! - `profile` writes the worker's Codex configuration and checks what Codex
//!   reports back.
//! - `draft` resolves what a hand-off would send, on this Mac, with no model.
//! - `events` reads what the worker did from the app-server's messages.
//! - `folder` makes the job folder and copies attached files into it.
//! - `session` runs one turn: `prepare` checks the plan and the profile, `drive` reads the stream.
//! - `store` keeps each job's record, ids only, in the state store.
//! - `job` runs one hand-off from a reviewed draft to a checked record.
//! - `manifest` is the worker's final message; `verify` is FNDR's check of it.
pub mod draft;
pub mod events;
pub mod folder;
pub mod job;
pub mod manifest;
pub mod profile;
pub mod session;
pub mod store;
pub mod verify;
