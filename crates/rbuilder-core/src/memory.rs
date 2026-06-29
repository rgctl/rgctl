//! Memory monitoring utilities for tracking resource usage during analysis.

use sysinfo::{Pid, ProcessRefreshKind, RefreshKind, System};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Monitor for tracking memory usage throughout the analysis pipeline.
pub struct MemoryMonitor {
    system: Arc<Mutex<System>>,
    start_time: Instant,
    start_memory: u64,
    peak_memory: Arc<Mutex<u64>>,
    pid: Pid,
}

impl MemoryMonitor {
    /// Create a new memory monitor that tracks the current process.
    pub fn new() -> Self {
        let mut system = System::new_with_specifics(
            RefreshKind::new().with_processes(ProcessRefreshKind::everything()),
        );
        system.refresh_all();

        let pid = sysinfo::get_current_pid().expect("Failed to get current PID");
        let start_memory = Self::current_memory(&system, pid);

        Self {
            system: Arc::new(Mutex::new(system)),
            start_time: Instant::now(),
            start_memory,
            peak_memory: Arc::new(Mutex::new(start_memory)),
            pid,
        }
    }

    /// Get current memory usage in bytes for the specified process.
    fn current_memory(system: &System, pid: Pid) -> u64 {
        if let Some(process) = system.process(pid) {
            process.memory() * 1024 // Convert from KB to bytes
        } else {
            0
        }
    }

    /// Take a snapshot of current memory usage.
    pub fn snapshot(&self) -> MemorySnapshot {
        let mut system = self.system.lock().unwrap();
        system.refresh_processes_specifics(ProcessRefreshKind::new().with_memory());

        let current = Self::current_memory(&system, self.pid);

        let mut peak = self.peak_memory.lock().unwrap();
        if current > *peak {
            *peak = current;
        }

        MemorySnapshot {
            current_mb: (current / 1024 / 1024) as f64,
            peak_mb: (*peak / 1024 / 1024) as f64,
            delta_mb: ((current as i64 - self.start_memory as i64) / 1024 / 1024) as f64,
            elapsed: self.start_time.elapsed(),
        }
    }

    /// Generate a human-readable memory report.
    pub fn report(&self) -> String {
        let snap = self.snapshot();
        format!(
            "Memory: {:.1}MB current, {:.1}MB peak ({:+.1}MB) @ {:.1}s",
            snap.current_mb,
            snap.peak_mb,
            snap.delta_mb,
            snap.elapsed.as_secs_f64()
        )
    }

    /// Get the current memory usage in MB.
    pub fn current_mb(&self) -> f64 {
        self.snapshot().current_mb
    }

    /// Get the peak memory usage in MB.
    pub fn peak_mb(&self) -> f64 {
        self.snapshot().peak_mb
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// A snapshot of memory usage at a point in time.
#[derive(Debug, Clone)]
pub struct MemorySnapshot {
    /// Current memory usage in MB
    pub current_mb: f64,
    /// Peak memory usage in MB since monitor started
    pub peak_mb: f64,
    /// Change from start in MB (can be negative)
    pub delta_mb: f64,
    /// Time elapsed since monitor started
    pub elapsed: std::time::Duration,
}

impl MemorySnapshot {
    /// Format as a short status string.
    pub fn to_string_short(&self) -> String {
        format!("{:.1}MB", self.current_mb)
    }

    /// Format as a detailed status string.
    pub fn to_string_detailed(&self) -> String {
        format!(
            "{:.1}MB current, {:.1}MB peak ({:+.1}MB)",
            self.current_mb, self.peak_mb, self.delta_mb
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_monitor_creation() {
        let monitor = MemoryMonitor::new();
        let snapshot = monitor.snapshot();
        assert!(snapshot.current_mb > 0.0, "Should have non-zero memory usage");
        assert!(snapshot.peak_mb >= snapshot.current_mb);
    }

    #[test]
    fn test_memory_monitor_report() {
        let monitor = MemoryMonitor::new();
        let report = monitor.report();
        assert!(report.contains("Memory:"));
        assert!(report.contains("MB"));
    }

    #[test]
    fn test_memory_snapshot_formatting() {
        let snapshot = MemorySnapshot {
            current_mb: 123.4,
            peak_mb: 150.0,
            delta_mb: 23.4,
            elapsed: std::time::Duration::from_secs(10),
        };

        let short = snapshot.to_string_short();
        assert_eq!(short, "123.4MB");

        let detailed = snapshot.to_string_detailed();
        assert!(detailed.contains("123.4MB current"));
        assert!(detailed.contains("150.0MB peak"));
    }
}
