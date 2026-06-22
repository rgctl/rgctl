//! Workspace CLI commands (Task 7.1)

use crate::multi_repo::sync::sync_workspace;
use crate::multi_repo::workspace::WorkspaceManifest;
use rbuilder_error::Result;
use std::path::{Path, PathBuf};

/// Initialize an empty workspace manifest.
pub fn run_init(workspace_root: &Path) -> Result<()> {
    let manifest = WorkspaceManifest::default();
    let path = manifest.save(workspace_root)?;
    println!("Created workspace manifest at {}", path.display());
    println!("Add repos with: rbuilder workspace add <path> --namespace <name>");
    Ok(())
}

/// Add a repository to the workspace.
pub fn run_add(workspace_root: &Path, repo_path: &Path, namespace: &str) -> Result<()> {
    let mut manifest = WorkspaceManifest::load(workspace_root)?;
    let rel = relative_path(workspace_root, repo_path)?;
    manifest.add_repo(namespace, rel)?;
    manifest.save(workspace_root)?;
    println!("Added repo '{namespace}' at {}", repo_path.display());
    Ok(())
}

/// List workspace repositories.
pub fn run_list(workspace_root: &Path) -> Result<()> {
    let manifest = WorkspaceManifest::load(workspace_root)?;
    if manifest.repos.is_empty() {
        println!("No repos in workspace. Run `rbuilder workspace add`.");
        return Ok(());
    }
    println!("Workspace repos ({}):", manifest.repos.len());
    for entry in &manifest.repos {
        let resolved = manifest.resolve_path(entry, workspace_root);
        println!("  - {} → {}", entry.namespace, resolved.display());
    }
    Ok(())
}

/// Sync all workspace repos into a merged graph.
pub fn run_sync(workspace_root: &Path, verbose: bool) -> Result<()> {
    let (graph, report) = sync_workspace(workspace_root, verbose)?;
    println!("Synced {} repo(s)", report.repos_indexed);
    println!(
        "Merged graph: {} nodes, {} edges",
        report.nodes, report.edges
    );
    println!(
        "Cross-repo links: {} edge(s) across {} pair(s)",
        report.cross_repo.edges_added,
        report.cross_repo.repo_pairs.len()
    );
    println!("Time: {:.2}s", report.duration_secs);
    println!(
        "Graph saved to {}/.rbuilder/graph.json",
        workspace_root.display()
    );
    let _ = graph;
    Ok(())
}

/// Remove a repo from the workspace manifest.
pub fn run_remove(workspace_root: &Path, namespace: &str) -> Result<()> {
    let mut manifest = WorkspaceManifest::load(workspace_root)?;
    if manifest.remove_repo(namespace) {
        manifest.save(workspace_root)?;
        println!("Removed repo '{namespace}' from workspace");
    } else {
        return Err(rbuilder_error::Error::NotFound(format!(
            "Namespace not found: {namespace}"
        )));
    }
    Ok(())
}

fn relative_path(base: &Path, target: &Path) -> Result<PathBuf> {
    let base = base.canonicalize().unwrap_or_else(|_| base.to_path_buf());
    let target = target
        .canonicalize()
        .unwrap_or_else(|_| target.to_path_buf());
    Ok(target
        .strip_prefix(&base)
        .map(|p| p.to_path_buf())
        .unwrap_or(target))
}
