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
pub use pr_scope::{
    DEFAULT_BASE_ARTIFACT_SUBDIR, EntityScope, HunkIndex, LineRange, PrCheckArtifactPaths,
    RGCTL_BASE_ARTIFACT_ENV, ScopedPaths, SymbolScopeOptions, changed_function_symbols,
    function_symbols_in_paths, git_diff_name_only, git_diff_name_status, git_diff_worktree_vs_head,
    git_rev_list_reverse, git_unified_diff, git_unified_diff_worktree, parse_name_status_z,
    resolve_base_artifact_root, resolve_base_snapshot, resolve_pr_check_artifacts,
    resolve_snapshot_path,
};
pub use rgctl_extraction::{
    FACTS_CACHE_REL, FactsCache, FactsKey, facts_key_for, facts_key_for_hash,
};
pub use synthesize_head::{
    HeadSynthesisOptions, default_synth_head_dir, out_root_clobbers_live_session,
    synthesize_head_snapshot, synthesize_head_snapshot_at, synthesize_worktree_head_snapshot,
    synthesize_worktree_head_snapshot_at,
};
pub use updater::{IncrementalUpdater, UpdateOptions, UpdateResult};

mod template_update;

pub use template_update::recover_template_dependencies;
