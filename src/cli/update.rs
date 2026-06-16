//! Incremental update command (Task 5.1.3)

use crate::discovery::DiscoveryConfig;
use crate::graph::CodeGraph;
use crate::incremental::{FileTracker, IncrementalUpdater, UpdateOptions, UpdateResult};
use crate::languages::registry::LanguageRegistry;
use crate::pipeline::{PipelineConfig, ProcessingPipeline};
use std::path::Path;
use std::sync::Arc;

/// Run the `rbuilder update` command.
pub fn run_update(repo_root: &Path, since: Option<String>, force: bool, verbose: bool) -> anyhow::Result<()> {
    if force {
        println!("Force rebuild requested...");
        let pipeline = ProcessingPipeline::with_config(
            Arc::new(LanguageRegistry::new()),
            PipelineConfig {
                show_progress: !verbose,
                ..PipelineConfig::default()
            },
        );

        let (graph, stats) = pipeline.process_repository(repo_root)?;
        graph.save_to_repo(repo_root)?;

        let mut tracker = FileTracker::new(repo_root);
        let files = crate::discovery::FileDiscoverer::new(Arc::new(LanguageRegistry::new()))
            .discover(repo_root)?;
        tracker.index_files(&files, &graph)?;
        tracker.save()?;

        print_summary(
            &UpdateResult {
                files_changed: stats.files_processed,
                nodes_added: stats.nodes_created,
                edges_added: stats.edges_created,
                duration: stats.duration,
                ..Default::default()
            },
            true,
        );
        return Ok(());
    }

    let graph = match CodeGraph::load_from_repo(repo_root) {
        Ok(graph) => graph,
        Err(_) => {
            println!("No existing graph found. Running initial index...");
            let pipeline = ProcessingPipeline::with_config(
                Arc::new(LanguageRegistry::new()),
                PipelineConfig {
                    show_progress: !verbose,
                    ..PipelineConfig::default()
                },
            );
            let (graph, stats) = pipeline.process_repository(repo_root)?;
            graph.save_to_repo(repo_root)?;

            let mut tracker = FileTracker::new(repo_root);
            let files = crate::discovery::FileDiscoverer::new(Arc::new(LanguageRegistry::new()))
                .discover(repo_root)?;
            tracker.index_files(&files, &graph)?;
            tracker.save()?;

            print_summary(
                &UpdateResult {
                    files_changed: stats.files_processed,
                    nodes_added: stats.nodes_created,
                    edges_added: stats.edges_created,
                    duration: stats.duration,
                    ..Default::default()
                },
                true,
            );
            return Ok(());
        }
    };

    let mut graph = graph;
    let updater = IncrementalUpdater::with_options(
        Arc::new(LanguageRegistry::new()),
        UpdateOptions {
            since,
            force: false,
            discovery: DiscoveryConfig::default(),
            show_progress: !verbose,
            ..Default::default()
        },
    );

    let result = updater.update(&mut graph, repo_root)?;
    print_summary(&result, false);
    Ok(())
}

fn print_summary(result: &UpdateResult, full: bool) {
    if full {
        println!("Full index complete");
    } else if result.files_affected() == 0 {
        println!("No changes detected");
        println!("Time: {:.2}s", result.duration.as_secs_f64());
        return;
    } else {
        println!("Detected {} changed file(s)", result.files_affected());
    }

    if result.files_added > 0 {
        println!("  Added: {}", result.files_added);
    }
    if result.files_changed > 0 {
        println!("  Modified: {}", result.files_changed);
    }
    if result.files_deleted > 0 {
        println!("  Deleted: {}", result.files_deleted);
    }
    if result.nodes_removed > 0 {
        println!("Removed {} node(s)", result.nodes_removed);
    }
    if result.nodes_added > 0 {
        println!("Updated {} node(s)", result.nodes_added);
    }
    if result.edges_added > 0 || result.edges_removed > 0 {
        println!(
            "Edges: +{} / -{}",
            result.edges_added, result.edges_removed
        );
    }
    println!("Time: {:.2}s", result.duration.as_secs_f64());
}
