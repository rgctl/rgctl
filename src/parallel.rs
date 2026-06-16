//! Parallel execution helpers (Phase 8.1)

use rayon::prelude::*;

/// Run a parallel iterator over `items`, optionally on a dedicated thread pool.
pub fn par_map<T, R, F>(thread_count: Option<usize>, items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync + Send,
{
    if let Some(n) = thread_count {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build()
            .expect("failed to build rayon thread pool");
        pool.install(|| items.par_iter().map(f).collect())
    } else {
        items.par_iter().map(f).collect()
    }
}

/// Run a parallel iterator and keep only `Some` results.
pub fn par_filter_map<T, R, F>(thread_count: Option<usize>, items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> Option<R> + Sync + Send,
{
    if let Some(n) = thread_count {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(n)
            .build()
            .expect("failed to build rayon thread pool");
        pool.install(|| items.par_iter().filter_map(f).collect())
    } else {
        items.par_iter().filter_map(f).collect()
    }
}
