//! Structured diff engine — tree-sitter symbol-level diffs (docs/prd.md §2).
//!
//! One structured-diff object feeds both the agent-facing MCP `diff_symbol`
//! tool and the future human PR view. Callers (the agent gateway, later the
//! API gateway) fetch before/after contents from clotho-vcs and hand them
//! here; this crate never touches storage itself.

pub mod engine;
pub mod service;

pub use engine::{diff_file, ChangeStatus, DiffError, FileDiff, SymbolChange};
pub use service::DiffService;
