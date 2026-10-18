//! Privacy module

mod blocklist;
pub mod erase;
pub mod export;
pub mod recording;
pub mod exclusions;
pub mod safety_gate;
pub mod vault;

#[cfg(test)]
mod default_exclusions_tests;
#[cfg(test)]
mod vault_tests;

pub use blocklist::Blocklist;
