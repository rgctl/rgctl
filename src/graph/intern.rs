//! String interning for memory-efficient graph storage
//!
//! Task 5.2.2: Deduplicate repeated strings across nodes

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Deduplicates strings to reduce memory usage for large graphs.
#[derive(Debug, Default, Clone)]
pub struct StringInterner {
    pool: Arc<RwLock<HashMap<String, Arc<str>>>>,
}

impl StringInterner {
    /// Create a new empty interner.
    pub fn new() -> Self {
        Self::default()
    }

    /// Intern a string, returning a shared handle.
    pub fn intern(&self, value: &str) -> Arc<str> {
        // Fast path: check if already interned (read lock)
        if let Ok(read) = self.pool.read() {
            if let Some(existing) = read.get(value) {
                return existing.clone();
            }
        }

        // Slow path: insert if not present (write lock with entry API)
        self.pool
            .write()
            .unwrap()
            .entry(value.to_string())
            .or_insert_with(|| Arc::from(value))
            .clone()
    }

    /// Intern a owned string in-place.
    pub fn intern_string(&self, value: &mut String) {
        let arc = self.intern(value);
        if value.as_str() != arc.as_ref() {
            *value = arc.to_string();
        }
    }

    /// Number of unique interned strings.
    pub fn len(&self) -> usize {
        self.pool.read().map(|p| p.len()).unwrap_or(0)
    }

    /// Whether the pool is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intern_deduplicates() {
        let interner = StringInterner::new();
        let a = interner.intern("hello");
        let b = interner.intern("hello");
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(interner.len(), 1);
    }
}
