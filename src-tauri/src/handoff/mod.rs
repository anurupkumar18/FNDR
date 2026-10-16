//! Hand-off (ADR 032): a person-approved job where a sandboxed Codex worker
//! turns a task plus attached memories and files into finished files in one
//! job folder, checked by FNDR before anything is shown as done.
//!
//! - `profile` writes the worker's Codex configuration and checks what Codex
//!   reports back.
//! - `draft` resolves what a hand-off would send, on this Mac, with no model.
//! - `folder` makes the job folder and copies attached files into it.
//! - `store` keeps each job's record, ids only, in the state store.
//! - `manifest` is the worker's final message; `verify` is FNDR's check of it.
pub mod draft;
pub mod folder;
pub mod manifest;
pub mod profile;
pub mod store;
pub mod verify;
