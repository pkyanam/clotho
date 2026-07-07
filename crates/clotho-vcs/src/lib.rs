//! Clotho VCS engine — jj-native, git-compatible version control as a service.
//!
//! Embeds `jj-lib` directly (git backend via gitoxide); never shells out to
//! the `jj` or `git` binaries. See `engine` for the design notes.

pub mod engine;
pub mod service;

pub use engine::VcsEngine;
pub use service::VcsService;
