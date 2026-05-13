//! Insight-first memory helpers.
//!
//! - [`derive_insight_for_record`]: fills persisted insight columns from structured
//!   fields + salience (OCR used only inside derivation, not copied into embedding text).
//! - [`compose_insight_embedding_text`]: builds `embedding_text` for the embedder with
//!   **no raw OCR** segments (see ADR 007).

mod derive;
mod embedding_text;

pub use derive::derive_insight_for_record;
pub use embedding_text::compose_insight_embedding_text;
