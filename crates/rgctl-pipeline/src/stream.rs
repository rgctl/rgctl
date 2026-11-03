//! Bounded-channel streaming between parallel extractors and sequential graph merge.

use crate::parallel::with_pool;
use crossbeam::channel::{Receiver, bounded};
use rayon::prelude::*;
use rgctl_error::Result;
use rgctl_extraction::{ExtractionTail, Extractor, FileExtraction, GraphBuilder};
use rgctl_registry::LanguageRegistry;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Default in-flight extraction cap (~1024 file buffers max between extract and merge).
pub const DEFAULT_STREAM_CHANNEL_CAPACITY: usize = 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtractionFailure {
    pub path: PathBuf,
    pub error: String,
}

/// CPU-sum / wall sub-timings for [`stream_into_graph`] (Instant only; no extra I/O).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ExtractPhaseTimings {
    /// Sum of `fs::read` across worker threads (can exceed wall).
    pub read_cpu: Duration,
    /// Sum of plugin extract (`extract_file_with_source`) across workers (can exceed wall).
    pub parse_cpu: Duration,
    /// Sequential pass-1 merge wall on the consumer thread.
    pub pass1_wall: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreamStats {
    pub files_processed: usize,
    pub extraction_failures: Vec<ExtractionFailure>,
    pub extract_phases: ExtractPhaseTimings,
    /// Absolute path string → BLAKE3 hex from extract workers (feeds FileTracker).
    pub file_hashes: HashMap<String, String>,
    /// Declined candidate path -> (worker content hash, final language). Not indexed.
    pub excluded_classifications: std::collections::BTreeMap<String, (String, String)>,
}

/// Run parallel extractors into a bounded channel while the caller consumes on the main thread.
pub fn start_parallel_extraction(
    thread_count: Option<usize>,
    extractor: Arc<Extractor>,
    files: Arc<Vec<PathBuf>>,
    capacity: usize,
    on_file_done: impl Fn() + Send + Sync + 'static,
    read_ns: Arc<AtomicU64>,
    parse_ns: Arc<AtomicU64>,
) -> Receiver<std::result::Result<FileExtraction, ExtractionFailure>> {
    let (tx, rx) = bounded(capacity);
    std::thread::spawn(move || {
        with_pool(thread_count, || {
            files.par_iter().for_each(|path| {
                let read_start = Instant::now();
                let read_result = std::fs::read(path);
                read_ns.fetch_add(
                    duration_as_nanos_u64(read_start.elapsed()),
                    Ordering::Relaxed,
                );
                match read_result {
                    Ok(source) => {
                        let parse_start = Instant::now();
                        let extracted = extractor.extract_file_with_source(path, source);
                        parse_ns.fetch_add(
                            duration_as_nanos_u64(parse_start.elapsed()),
                            Ordering::Relaxed,
                        );
                        match extracted {
                            Ok(extraction) => {
                                let _ = tx.send(Ok(extraction));
                            }
                            Err(err) => {
                                let _ = tx.send(Err(ExtractionFailure {
                                    path: path.clone(),
                                    error: err.to_string(),
                                }));
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(ExtractionFailure {
                            path: path.clone(),
                            error: err.to_string(),
                        }));
                    }
                }
                on_file_done();
            });
        });
    });
    rx
}

/// Extract in parallel, merge pass-1 immediately, and retain only relation tails for pass 2.
pub fn stream_into_graph(
    thread_count: Option<usize>,
    extractor: &Extractor,
    _registry: Arc<LanguageRegistry>,
    files: &[PathBuf],
    capacity: usize,
    builder: &mut GraphBuilder,
    on_file_done: impl Fn() + Send + Sync + 'static,
) -> Result<(StreamStats, Vec<ExtractionTail>)> {
    let files = Arc::new(files.to_vec());
    let file_count = files.len();
    let read_ns = Arc::new(AtomicU64::new(0));
    let parse_ns = Arc::new(AtomicU64::new(0));
    let rx = start_parallel_extraction(
        thread_count,
        Arc::new(extractor.clone()),
        files,
        capacity,
        on_file_done,
        Arc::clone(&read_ns),
        Arc::clone(&parse_ns),
    );

    let mut tails = Vec::with_capacity(file_count);
    let mut stats = StreamStats::default();
    stats.file_hashes.reserve(file_count);
    let mut pass1_wall = Duration::ZERO;
    while let Ok(result) = rx.recv() {
        match result {
            Ok(mut extraction) => {
                if extraction.excluded {
                    if let (Some(hash), Some(language)) =
                        (extraction.file_hash, extraction.language)
                    {
                        stats.excluded_classifications.insert(
                            extraction.path.to_string_lossy().into_owned(),
                            (hash, language),
                        );
                    }
                    continue;
                }
                if let Some(hash) = extraction.file_hash.take() {
                    stats
                        .file_hashes
                        .insert(extraction.path.to_string_lossy().into_owned(), hash);
                }
                let pass1_start = Instant::now();
                tails.push(extractor.populate_pass1(&mut extraction, builder)?);
                pass1_wall += pass1_start.elapsed();
                stats.files_processed += 1;
            }
            Err(failure) => {
                stats.extraction_failures.push(failure);
            }
        }
    }

    stats.extract_phases = ExtractPhaseTimings {
        read_cpu: Duration::from_nanos(read_ns.load(Ordering::Relaxed)),
        parse_cpu: Duration::from_nanos(parse_ns.load(Ordering::Relaxed)),
        pass1_wall,
    };

    Ok((stats, tails))
}

fn duration_as_nanos_u64(d: Duration) -> u64 {
    u64::try_from(d.as_nanos()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stream_reports_extraction_failures() {
        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(Arc::clone(&registry));
        let mut builder = GraphBuilder::new();
        let missing = TempDir::new().unwrap().path().join("missing.rs");

        let (stats, tails) = stream_into_graph(
            Some(1),
            &extractor,
            registry,
            &[missing.clone()],
            8,
            &mut builder,
            || {},
        )
        .unwrap();

        assert_eq!(stats.files_processed, 0);
        assert_eq!(stats.extraction_failures.len(), 1);
        assert_eq!(stats.extraction_failures[0].path, missing);
        assert!(tails.is_empty());
    }

    #[test]
    fn stream_records_extract_phase_timings() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("ok.rs");
        std::fs::write(&path, "fn hello() {}\n").unwrap();

        let registry = Arc::new(rgctl_languages::default_registry());
        let extractor = Extractor::new(Arc::clone(&registry));
        let mut builder = GraphBuilder::new();

        let (stats, tails) = stream_into_graph(
            Some(1),
            &extractor,
            registry,
            &[path],
            8,
            &mut builder,
            || {},
        )
        .unwrap();

        assert_eq!(stats.files_processed, 1);
        assert_eq!(tails.len(), 1);
        assert!(stats.extract_phases.parse_cpu > Duration::ZERO);
        assert!(stats.extract_phases.pass1_wall > Duration::ZERO);
    }
}

/// Drain eligible literal targets through deduplicated worker waves, reusing the
/// discovery inventory. Each file is read/extracted once and ASTs stay on workers.
pub fn stream_inventory_into_graph(
    thread_count: Option<usize>,
    extractor: &Extractor,
    registry: Arc<LanguageRegistry>,
    inventory: &rgctl_extraction::discovery::DiscoveryInventory,
    capacity: usize,
    builder: &mut GraphBuilder,
    on_file_done: impl Fn() + Send + Sync + 'static,
) -> Result<(StreamStats, Vec<ExtractionTail>)> {
    let (mut stats, mut tails) = stream_into_graph(
        thread_count,
        extractor,
        Arc::clone(&registry),
        &inventory.files,
        capacity,
        builder,
        on_file_done,
    )?;
    let mut remaining: std::collections::BTreeMap<_, _> =
        inventory.template_candidates.iter().cloned().collect();
    loop {
        let wanted = builder.template_candidate_paths()?;
        let selected: Vec<_> = remaining
            .keys()
            .filter(|path| {
                wanted.contains(
                    &rgctl_graph::normalize_path_str(&path.to_string_lossy()).into_owned(),
                )
            })
            .cloned()
            .collect();
        if selected.is_empty() {
            break;
        }
        for path in &selected {
            remaining.remove(path);
        }
        let wave = extractor
            .clone()
            .with_template_targets(selected.iter().cloned().collect());
        let (next, extra) = stream_into_graph(
            thread_count,
            &wave,
            Arc::clone(&registry),
            &selected,
            capacity,
            builder,
            || {},
        )?;
        merge_stream_stats(&mut stats, next);
        tails.extend(extra);
    }
    let ordinary: Vec<_> = remaining
        .into_iter()
        .filter_map(|(path, selected)| selected.then_some(path))
        .collect();
    if !ordinary.is_empty() {
        let (next, extra) = stream_into_graph(
            thread_count,
            extractor,
            registry,
            &ordinary,
            capacity,
            builder,
            || {},
        )?;
        merge_stream_stats(&mut stats, next);
        tails.extend(extra);
    }
    Ok((stats, tails))
}
fn merge_stream_stats(stats: &mut StreamStats, next: StreamStats) {
    stats.files_processed += next.files_processed;
    stats.extraction_failures.extend(next.extraction_failures);
    stats.file_hashes.extend(next.file_hashes);
    stats
        .excluded_classifications
        .extend(next.excluded_classifications);
    stats.extract_phases.read_cpu += next.extract_phases.read_cpu;
    stats.extract_phases.parse_cpu += next.extract_phases.parse_cpu;
    stats.extract_phases.pass1_wall += next.extract_phases.pass1_wall;
}
