//! Incremental graph updates and change detection

pub mod cascade;
pub mod changes;
pub mod file_tracker;
pub mod pr_scope;
pub mod synthesize_head;
pub mod updater;

pub use cascade::{incoming_callers_files, incoming_callers_files_depth};
pub use changes::{ChangeDetail, ChangeDetectionResult, ChangeDetector, ChangeSummary};
pub use file_tracker::{
    ChangeSet, FileTracker, changes_for_paths, group_sorted_node_paths, merge_change_sets,
    normalize_path_str,
};
pub use synthesize_head::{
    HeadSynthesisOptions, default_synth_head_dir, out_root_clobbers_live_session,
    synthesize_head_snapshot, synthesize_head_snapshot_at, synthesize_worktree_head_snapshot,
    synthesize_worktree_head_snapshot_at,
};
pub use pr_scope::{
    EntityScope, HunkIndex, LineRange, PrCheckArtifactPaths, ScopedPaths, SymbolScopeOptions,
    changed_function_symbols, function_symbols_in_paths, git_diff_name_only, git_diff_name_status,
    git_diff_worktree_vs_head, git_rev_list_reverse, git_unified_diff, git_unified_diff_worktree,
    parse_name_status_z, resolve_base_artifact_root, resolve_base_snapshot,
    resolve_pr_check_artifacts, resolve_snapshot_path,
    DEFAULT_BASE_ARTIFACT_SUBDIR, RGCTL_BASE_ARTIFACT_ENV,
};
pub use updater::{IncrementalUpdater, UpdateOptions, UpdateResult};
