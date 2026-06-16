//! Graph backend implementations

pub mod indradb;
pub mod memory;
pub mod trait_def;

pub use memory::MemoryBackend;
pub use trait_def::GraphBackend;
