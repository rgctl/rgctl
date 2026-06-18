//! Incremental graph updates

pub mod file_tracker;
pub mod updater;

pub use file_tracker::{
    changes_for_paths, ChangeSet, FileHashMetadata, FileTracker, FILE_HASHES_FILE,
};
pub use updater::{IncrementalUpdater, UpdateOptions, UpdateResult};
