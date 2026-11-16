//! Parallel execution helpers (Phase 8.1)

use rayon::ThreadPool;
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

/// Default Rayon worker stack (OS default, typically ~2 MiB on macOS).
/// Used for extract, preload, and other shallow work.
const _DEFAULT_WORKER_STACK_SIZE: usize = 0; // rayon default

/// CFG / field-write workers that may still hit deep native stacks before iterative
/// lowering completes on every path.
const LARGE_WORKER_STACK_SIZE: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum PoolStack {
    Default,
    Large,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct PoolKey {
    threads: Option<usize>,
    stack: PoolStack,
}

static POOL_REGISTRY: OnceLock<Mutex<HashMap<PoolKey, Arc<ThreadPool>>>> = OnceLock::new();

fn registry() -> &'static Mutex<HashMap<PoolKey, Arc<ThreadPool>>> {
    POOL_REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

fn get_pool(key: PoolKey) -> Arc<ThreadPool> {
    let mut lock = registry()
        .lock()
        .expect("rayon pool registry lock poisoned");
    if let Some(pool) = lock.get(&key) {
        return Arc::clone(pool);
    }
    let mut builder =
        rayon::ThreadPoolBuilder::new().thread_name(|idx| format!("rgctl-worker-{idx}"));
    match key.stack {
        // DEFAULT_WORKER_STACK_SIZE is 0 → rayon default stack.
        PoolStack::Default => {}
        PoolStack::Large => {
            builder = builder.stack_size(LARGE_WORKER_STACK_SIZE);
        }
    }
    if let Some(n) = key.threads {
        builder = builder.num_threads(n);
    }
    let pool = Arc::new(builder.build().expect("failed to build rayon thread pool"));
    lock.insert(key, Arc::clone(&pool));
    pool
}

/// Return a process-wide Rayon pool for `thread_count` (`None` = global default pool).
///
/// Workers use the OS default stack size.
pub fn thread_pool(thread_count: Option<usize>) -> Arc<ThreadPool> {
    get_pool(PoolKey {
        threads: thread_count,
        stack: PoolStack::Default,
    })
}

/// Rayon pool with [`LARGE_WORKER_STACK_SIZE`] for CFG / analysis passes.
pub fn large_stack_thread_pool(thread_count: Option<usize>) -> Arc<ThreadPool> {
    get_pool(PoolKey {
        threads: thread_count,
        stack: PoolStack::Large,
    })
}

/// Run `f` on the default-stack pooled Rayon executor for `thread_count`.
pub fn with_pool<R, F>(thread_count: Option<usize>, f: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    thread_pool(thread_count).install(f)
}

/// Run `f` on the large-stack Rayon pool (CFG batch workers).
pub fn with_large_pool<R, F>(thread_count: Option<usize>, f: F) -> R
where
    F: FnOnce() -> R + Send,
    R: Send,
{
    large_stack_thread_pool(thread_count).install(f)
}

/// Run `f` on a scoped thread with [`LARGE_WORKER_STACK_SIZE`].
///
/// Rayon `install` also runs work on the calling thread; CFG passes use this so that
/// coordinator is not limited to the main thread's ~2 MiB stack.
pub fn with_large_stack<R>(f: impl FnOnce() -> R + Send) -> R
where
    R: Send,
{
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("rgctl-large-stack".to_string())
            .stack_size(LARGE_WORKER_STACK_SIZE)
            .spawn_scoped(scope, f)
            .expect("failed to spawn large-stack thread")
            .join()
            .expect("large-stack thread panicked")
    })
}

/// Run a parallel iterator over `items`, optionally on a dedicated thread pool.
pub fn par_map<T, R, F>(thread_count: Option<usize>, items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> R + Sync + Send,
{
    with_pool(thread_count, || items.par_iter().map(f).collect())
}

/// Run a parallel iterator and keep only `Some` results.
pub fn par_filter_map<T, R, F>(thread_count: Option<usize>, items: &[T], f: F) -> Vec<R>
where
    T: Sync,
    R: Send,
    F: Fn(&T) -> Option<R> + Sync + Send,
{
    with_pool(thread_count, || items.par_iter().filter_map(f).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_pool_workers_use_configured_stack() {
        with_large_pool(None, || {
            (0..4).into_par_iter().for_each(|_| {
                let mut buf = [0u8; 4 * 1024 * 1024];
                buf[0] = 1;
                std::hint::black_box(&buf);
            });
        });
    }

    #[test]
    fn default_and_large_pools_are_distinct() {
        let default_pool = thread_pool(None);
        let large_pool = large_stack_thread_pool(None);
        assert!(!Arc::ptr_eq(&default_pool, &large_pool));
    }
}
