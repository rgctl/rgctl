//! Memory monitoring utilities for tracking resource usage during analysis.

use rgctl_error::{Error, Result};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, System};

/// Monitor for tracking memory usage throughout the analysis pipeline.
pub struct MemoryMonitor {
    system: Arc<Mutex<System>>,
    start_time: Instant,
    start_memory: u64,
    /// Absolute high-water mark since monitor creation.
    peak_memory: Arc<AtomicU64>,
    /// High-water mark for the current sealed phase (ingest vs analysis).
    phase_peak: Arc<AtomicU64>,
    pid: Pid,
    sampler_stop: Option<Arc<AtomicBool>>,
    sampler_handle: Option<JoinHandle<()>>,
}

impl MemoryMonitor {
    /// Create a new memory monitor that tracks the current process.
    pub fn new() -> Self {
        Self::try_new().expect("Failed to create MemoryMonitor")
    }

    /// Create a monitor, propagating PID lookup failures.
    pub fn try_new() -> Result<Self> {
        let pid = sysinfo::get_current_pid()
            .map_err(|e| Error::Other(format!("Failed to get current PID: {e}")))?;
        let mut system = System::new();
        system.refresh_pids_specifics(&[pid], ProcessRefreshKind::new().with_memory());
        let start_memory = Self::current_memory(&system, pid);

        Ok(Self {
            system: Arc::new(Mutex::new(system)),
            start_time: Instant::now(),
            start_memory,
            peak_memory: Arc::new(AtomicU64::new(start_memory)),
            phase_peak: Arc::new(AtomicU64::new(start_memory)),
            pid,
            sampler_stop: None,
            sampler_handle: None,
        })
    }

    /// Start a background sampler that updates peak RSS every `interval`.
    ///
    /// Call once near the start of a long discover run so `peak_mb` reflects the
    /// true high-water mark, not only explicit [`Self::snapshot`] call sites.
    pub fn start_periodic_sampling(&mut self, interval: Duration) {
        if self.sampler_handle.is_some() {
            return;
        }
        let stop = Arc::new(AtomicBool::new(false));
        let system = Arc::clone(&self.system);
        let peak = Arc::clone(&self.peak_memory);
        let phase = Arc::clone(&self.phase_peak);
        let pid = self.pid;
        let stop_flag = Arc::clone(&stop);
        self.sampler_stop = Some(stop);
        self.sampler_handle = Some(thread::spawn(move || {
            while !stop_flag.load(Ordering::Acquire) {
                if let Ok(mut sys) = system.lock() {
                    sys.refresh_pids_specifics(&[pid], ProcessRefreshKind::new().with_memory());
                    let current = Self::current_memory(&sys, pid);
                    peak.fetch_max(current, Ordering::Relaxed);
                    phase.fetch_max(current, Ordering::Relaxed);
                }
                if stop_flag.load(Ordering::Acquire) {
                    break;
                }
                thread::park_timeout(interval);
            }
        }));
    }

    /// Stop the background sampler (if any) and take a final peak sample.
    pub fn stop_periodic_sampling(&mut self) {
        if let Some(stop) = self.sampler_stop.take() {
            stop.store(true, Ordering::Release);
        }
        if let Some(handle) = self.sampler_handle.take() {
            handle.thread().unpark();
            let _ = handle.join();
        }
        let _ = self.snapshot();
    }

    /// Get current memory usage in bytes for the specified process.
    fn current_memory(system: &System, pid: Pid) -> u64 {
        if let Some(process) = system.process(pid) {
            process.memory() // Already in bytes
        } else {
            0
        }
    }

    fn refresh_peaks(&self) -> Result<(u64, u64, u64)> {
        let mut system = self
            .system
            .lock()
            .map_err(|e| Error::GraphError(format!("MemoryMonitor system lock poisoned: {e}")))?;
        system.refresh_pids_specifics(&[self.pid], ProcessRefreshKind::new().with_memory());
        let current = Self::current_memory(&system, self.pid);
        self.peak_memory.fetch_max(current, Ordering::Relaxed);
        self.phase_peak.fetch_max(current, Ordering::Relaxed);
        Ok((
            current,
            self.peak_memory.load(Ordering::Relaxed).max(current),
            self.phase_peak.load(Ordering::Relaxed).max(current),
        ))
    }

    /// Seal the current phase: return its peak RSS in MB and reset the phase peak to
    /// *current* RSS so the next phase (e.g. analysis after ingest) measures independently.
    ///
    /// Absolute [`Self::peak_mb`] is unchanged — it remains the run-wide high-water mark.
    pub fn seal_phase(&self) -> Result<f64> {
        let (current, _, phase) = self.refresh_peaks()?;
        let phase_mb = (phase / 1024 / 1024) as f64;
        // Reset phase peak to current so analysis doesn't inherit ingest high-water.
        self.phase_peak.store(current, Ordering::Relaxed);
        Ok(phase_mb)
    }

    /// Peak RSS in MB for the current (unsealed) phase.
    pub fn phase_peak_mb(&self) -> f64 {
        self.refresh_peaks()
            .map(|(_, _, phase)| (phase / 1024 / 1024) as f64)
            .unwrap_or(0.0)
    }

    /// Take a snapshot of current memory usage.
    pub fn snapshot(&self) -> Result<MemorySnapshot> {
        let (current, peak, _) = self.refresh_peaks()?;

        Ok(MemorySnapshot {
            current_mb: (current / 1024 / 1024) as f64,
            peak_mb: (peak / 1024 / 1024) as f64,
            delta_mb: ((current as i64 - self.start_memory as i64) / 1024 / 1024) as f64,
            elapsed: self.start_time.elapsed(),
        })
    }

    /// Generate a human-readable memory report.
    pub fn report(&self) -> String {
        match self.snapshot() {
            Ok(snap) => format!(
                "Memory: {:.1}MB current, {:.1}MB peak ({:+.1}MB) @ {:.1}s",
                snap.current_mb,
                snap.peak_mb,
                snap.delta_mb,
                snap.elapsed.as_secs_f64()
            ),
            Err(e) => format!("Memory: unavailable ({e})"),
        }
    }

    /// Get the current memory usage in MB.
    pub fn current_mb(&self) -> f64 {
        self.snapshot().map(|s| s.current_mb).unwrap_or(0.0)
    }

    /// Get the peak memory usage in MB.
    pub fn peak_mb(&self) -> f64 {
        self.snapshot().map(|s| s.peak_mb).unwrap_or(0.0)
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MemoryMonitor {
    fn drop(&mut self) {
        self.stop_periodic_sampling();
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
        let monitor = MemoryMonitor::try_new().unwrap();
        let snapshot = monitor.snapshot().unwrap();
        assert!(
            snapshot.current_mb > 0.0,
            "Should have non-zero memory usage"
        );
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

    #[test]
    fn periodic_sampler_updates_peak() {
        let mut monitor = MemoryMonitor::new();
        monitor.start_periodic_sampling(Duration::from_millis(50));
        thread::sleep(Duration::from_millis(120));
        monitor.stop_periodic_sampling();
        let snap = monitor.snapshot().unwrap();
        assert!(snap.peak_mb > 0.0);
    }

    #[test]
    fn seal_phase_resets_phase_peak_keeps_absolute() {
        let monitor = MemoryMonitor::new();
        let ingest = monitor.seal_phase().unwrap();
        assert!(ingest > 0.0);
        let after = monitor.phase_peak_mb();
        let absolute = monitor.peak_mb();
        assert!(absolute >= ingest);
        // Fresh phase peak should be at-or-below absolute high-water.
        assert!(after <= absolute + 1.0);
    }
}
