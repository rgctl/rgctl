//! String interning for memory-efficient graph storage
//!
//! Task 5.2.2: Deduplicate repeated strings across nodes

use rbuilder_error::{Error, Result};
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
    pub fn intern(&self, value: &str) -> Result<Arc<str>> {
        // Fast path: check if already interned (read lock)
        if let Ok(read) = self.pool.read() {
            if let Some(existing) = read.get(value) {
                return Ok(existing.clone());
            }
        }

        // Slow path: insert if not present (write lock with entry API)
        // Note: Race condition is acceptable - duplicate Arc<str> instances
        // will be deduplicated on next read, only slight temporary memory overhead
        Ok(self
            .pool
            .write()
            .map_err(|e| Error::GraphError(format!("StringInterner lock poisoned: {e}")))?
            .entry(value.to_string())
            .or_insert_with(|| Arc::from(value))
            .clone())
    }

    /// Ensure a string is in the intern pool.
    ///
    /// NOTE: This method currently doesn't optimize the in-memory representation
    /// since Node stores String, not Arc<str>. The real memory savings come from
    /// the indexes using Arc<str>. Future optimization: change Node to store Arc<str>.
    pub fn intern_string(&self, _value: &mut String) {
        // Intentionally does nothing - the actual interning happens when building indexes
        // This method exists to maintain API compatibility but should be reconsidered
        // in a future refactor where Node uses Arc<str> directly
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
        let a = interner.intern("hello").unwrap();
        let b = interner.intern("hello").unwrap();
        assert!(Arc::ptr_eq(&a, &b));
        assert_eq!(interner.len(), 1);
    }
}
