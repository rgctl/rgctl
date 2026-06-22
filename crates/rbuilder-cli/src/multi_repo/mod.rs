//! Multi-repository workspace support

pub mod linker;
pub mod sync;
pub mod workspace;

pub use linker::{link_cross_repo, CrossRepoLinkReport};
pub use sync::{sync_workspace, WorkspaceSyncReport};
pub use workspace::{
    load_workspace_graph, stamp_repo_namespace, RepoEntry, WorkspaceManifest, WORKSPACE_FILE,
};
