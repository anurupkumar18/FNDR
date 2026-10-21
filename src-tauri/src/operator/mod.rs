//! Notch Do: plans a spoken request, then carries it out step by step on the
//! Mac through FNDR-native tools and a computer-use MCP server, with every
//! action gated by `policy` and written to `journal`.

#[cfg(test)]
pub(crate) mod eval;
pub mod journal;
pub mod layout;
pub mod mcp;
pub mod memory;
pub mod native;
pub mod plan;
pub mod policy;
pub mod runs;
pub mod skills;
