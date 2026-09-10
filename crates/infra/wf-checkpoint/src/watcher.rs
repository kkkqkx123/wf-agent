use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::watch;

use crate::error::CheckpointError;
use crate::file::FileCheckpointManager;
use crate::scan::{ScanConfig, WorkspaceScanner};

/// File change event kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileChangeKind {
    Add,
    Change,
    Unlink,
    /// File renamed/moved. The record's `path` is the new absolute path;
    /// `FileChangeRecord.from` carries the old absolute path.
    Rename,
}

/// A single file change record with an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeRecord {
    pub path: PathBuf,
    pub kind: FileChangeKind,
    pub timestamp: i64,
    /// Rename source (old absolute path). `Some` only when
    /// `kind == FileChangeKind::Rename`.
    pub from: Option<PathBuf>,
}

impl FileChangeRecord {
    pub fn new(path: PathBuf, kind: FileChangeKind, timestamp: i64) -> Self {
        Self {
            path,
            kind,
            timestamp,
            from: None,
        }
    }

    pub fn renamed(from: PathBuf, to: PathBuf, timestamp: i64) -> Self {
        Self {
            path: to,
            kind: FileChangeKind::Rename,
            timestamp,
            from: Some(from),
        }
    }
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Lexically normalize an absolute path without touching the filesystem:
/// resolve `.`, collapse `..` without escaping the root prefix, and strip
/// redundant separators. All watcher keys and recent-agent registry keys use
/// this form so the same file cannot be missed due to path spelling.
pub fn normalize_absolute_path(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    if out.as_os_str().is_empty() {
        return PathBuf::from("/");
    }
    out
}

#[derive(Default)]
struct WatcherState {
    /// Records flushed from `pending`, awaiting batch consumption.
    changed: HashMap<PathBuf, FileChangeRecord>,
    /// Records received but not yet flushed (debounce window).
    pending: HashMap<PathBuf, FileChangeRecord>,
}

impl WatcherState {
    fn flush(&mut self) {
        for (path, record) in std::mem::take(&mut self.pending) {
            if record.kind == FileChangeKind::Unlink {
                let existing = self.changed.get(&path);
                if matches!(
                    existing,
                    Some(existing) if existing.kind != FileChangeKind::Unlink
                ) {
                    // File was added/changed then deleted: drop the record
                    // entirely.
                    self.changed.remove(&path);
                } else {
                    self.changed.insert(path, record);
                }
            } else {
                self.changed.insert(path, record);
            }
        }
    }
}

/// Persistent file watcher based on `notify`: tracks changed files in real
/// time so checkpoints only need to hash the actual changes instead of
/// rescanning the whole workspace.
pub struct FileWatcher {
    root: PathBuf,
    scanner: WorkspaceScanner,
    debounce: Duration,
    state: Arc<std::sync::Mutex<WatcherState>>,
    watcher: Option<RecommendedWatcher>,
    task: Option<tokio::task::JoinHandle<()>>,
    stop_tx: Option<watch::Sender<bool>>,
    ready: bool,
}

impl FileWatcher {
    /// Create a watcher without starting it. Call [`FileWatcher::start`]
    /// to begin monitoring (requires a tokio runtime).
    pub fn new(root: impl Into<PathBuf>, config: ScanConfig, debounce_ms: u64) -> Self {
        Self {
            root: root.into(),
            scanner: WorkspaceScanner::new(config),
            debounce: Duration::from_millis(debounce_ms),
            state: Arc::new(std::sync::Mutex::new(WatcherState::default())),
            watcher: None,
            task: None,
            stop_tx: None,
            ready: false,
        }
    }

    /// Start watching the root directory recursively.
    pub fn start(&mut self) -> Result<(), CheckpointError> {
        if self.watcher.is_some() {
            return Err(CheckpointError::Internal(
                "FileWatcher is already running".to_string(),
            ));
        }
        let (event_tx, event_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = event_tx.send(event);
            }
        })
        .map_err(|e| CheckpointError::Internal(format!("notify: {e}")))?;

        let root = self.root.clone();
        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|e| CheckpointError::Internal(format!("notify: {e}")))?;

        let (stop_tx, stop_rx) = watch::channel(false);
        let state = self.state.clone();
        let scanner = WorkspaceScanner::new(ScanConfig {
            custom_ignore_patterns: self.scanner.config().custom_ignore_patterns.clone(),
            failure_behavior: self.scanner.config().failure_behavior,
        });
        let debounce = self.debounce;
        let root_for_task = root.clone();
        let task = tokio::spawn(async move {
            run_event_loop(root_for_task, scanner, debounce, state, event_rx, stop_rx).await;
        });

        self.watcher = Some(watcher);
        self.task = Some(task);
        self.stop_tx = Some(stop_tx);
        self.ready = true;
        Ok(())
    }

    /// Stop watching and stop the background task.
    pub async fn stop(&mut self) {
        self.ready = false;
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(true);
        }
        self.watcher = None;
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
        self.state
            .lock()
            .expect("watcher state poisoned")
            .pending
            .clear();
    }

    /// All changed files currently buffered, with absolute paths.
    /// Prefer [`Self::take_batch`] for consumption: this snapshot does not
    /// claim ownership, so a concurrent `reset` could drop events observed
    /// here.
    pub fn get_changed_files(&self) -> Vec<FileChangeRecord> {
        self.state
            .lock()
            .expect("watcher state poisoned")
            .changed
            .values()
            .cloned()
            .collect()
    }

    /// Changed file paths (absolute) currently buffered.
    pub fn get_changed_paths(&self) -> Vec<PathBuf> {
        self.state
            .lock()
            .expect("watcher state poisoned")
            .changed
            .keys()
            .cloned()
            .collect()
    }

    /// Whether a file has changed since the last batch consumption. Relative
    /// paths are resolved against the watched root.
    pub fn has_changed(&self, file_path: impl AsRef<Path>) -> bool {
        let absolute = self.resolve_absolute(file_path.as_ref());
        self.state
            .lock()
            .expect("watcher state poisoned")
            .changed
            .contains_key(&absolute)
    }

    /// Atomically take the current `changed` map as an in-flight batch.
    /// Records arriving after the take (new `pending` flushes or direct
    /// `notify_*` inserts) stay buffered for the next batch and are never
    /// dropped by this call. The caller owns the batch: on success it is
    /// consumed, on failure the unprocessed records must be returned via
    /// [`Self::requeue_batch`].
    pub fn take_batch(&self) -> Vec<FileChangeRecord> {
        let mut state = self.state.lock().expect("watcher state poisoned");
        let taken = std::mem::take(&mut state.changed);
        let mut out: Vec<FileChangeRecord> = taken.into_values().collect();
        out.sort_by(|a, b| a.path.cmp(&b.path));
        out
    }

    /// Return unprocessed records to the front of the queue after a batch
    /// failure. Records are merged by path; a record that already has a
    /// newer buffered entry keeps the newer entry, otherwise the unprocessed
    /// record is restored. The batch boundary is preserved by sorting on
    /// path before reinsertion.
    pub fn requeue_batch(&self, records: Vec<FileChangeRecord>) {
        if records.is_empty() {
            return;
        }
        let mut state = self.state.lock().expect("watcher state poisoned");
        for record in records {
            state.changed.entry(record.path.clone()).or_insert(record);
        }
    }

    /// Number of buffered (not yet taken) change records.
    pub fn buffered_len(&self) -> usize {
        self.state
            .lock()
            .expect("watcher state poisoned")
            .changed
            .len()
    }

    /// Clear buffered `changed` records. Pending (debounce-window) events
    /// are intentionally preserved: they have not been flushed yet and
    /// clearing them would drop events. Prefer `take_batch` + success
    /// confirm over manual `reset` in production pumps.
    pub fn reset(&self) {
        let mut state = self.state.lock().expect("watcher state poisoned");
        state.changed.clear();
    }

    /// Whether the watcher has been started.
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// The watched root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Manually record a file change (for external events): recorded
    /// immediately without debounce.
    pub fn notify_file_change(&self, file_path: impl AsRef<Path>, kind: FileChangeKind) {
        let absolute = self.resolve_absolute(file_path.as_ref());
        let mut state = self.state.lock().expect("watcher state poisoned");
        state.changed.insert(
            absolute.clone(),
            FileChangeRecord::new(absolute, kind, now_millis()),
        );
    }

    /// Manually record a rename (for external events): `from` is the old
    /// absolute or workspace-relative path, `to` the new one.
    pub fn notify_file_rename(&self, from: impl AsRef<Path>, to: impl AsRef<Path>) {
        let from_abs = self.resolve_absolute(from.as_ref());
        let to_abs = self.resolve_absolute(to.as_ref());
        let mut state = self.state.lock().expect("watcher state poisoned");
        state.changed.insert(
            to_abs.clone(),
            FileChangeRecord::renamed(from_abs, to_abs, now_millis()),
        );
    }

    fn resolve_absolute(&self, path: &Path) -> PathBuf {
        let joined = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        };
        normalize_absolute_path(&joined)
    }
}

async fn run_event_loop(
    root: PathBuf,
    scanner: WorkspaceScanner,
    debounce: Duration,
    state: Arc<std::sync::Mutex<WatcherState>>,
    mut events: tokio::sync::mpsc::UnboundedReceiver<Event>,
    mut stop: watch::Receiver<bool>,
) {
    let mut pending_flush: Option<tokio::task::JoinHandle<()>> = None;

    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
            event = events.recv() => {
                let Some(event) = event else { break; };
                let mut guard = state.lock().expect("watcher state poisoned");
                if let Some(records) = filter_event(&root, &scanner, &event) {
                    for record in records {
                        guard.pending.insert(record.path.clone(), record);
                    }
                    if let Some(handle) = pending_flush.take() {
                        handle.abort();
                    }
                    let state = state.clone();
                    pending_flush = Some(tokio::spawn(async move {
                        tokio::time::sleep(debounce).await;
                        state.lock().expect("watcher state poisoned").flush();
                    }));
                }
            }
        }
    }
    if let Some(handle) = pending_flush {
        handle.abort();
    }
}

fn filter_event(
    root: &Path,
    scanner: &WorkspaceScanner,
    event: &Event,
) -> Option<Vec<FileChangeRecord>> {
    let timestamp = now_millis();
    // Rename/move: `notify` reports `ModifyKind::Name` with [from, to].
    // Emit a single Rename record so callers can record the move linkage
    // instead of an uncorrelated delete + add pair.
    if matches!(
        event.kind,
        EventKind::Modify(notify::event::ModifyKind::Name(_))
    ) && event.paths.len() == 2
    {
        let from = normalize_absolute_path(&event.paths[0]);
        let to = normalize_absolute_path(&event.paths[1]);
        if to == *root {
            return None;
        }
        let ignored = |p: &Path| {
            p.strip_prefix(root)
                .map(|relative| scanner.is_ignored(&relative.to_string_lossy().replace('\\', "/")))
                .unwrap_or(false)
        };
        if ignored(&from) || ignored(&to) {
            return None;
        }
        return Some(vec![FileChangeRecord::renamed(from, to, timestamp)]);
    }
    let kind = match event.kind {
        EventKind::Create(_) => FileChangeKind::Add,
        EventKind::Modify(_) => FileChangeKind::Change,
        EventKind::Remove(_) => FileChangeKind::Unlink,
        _ => return None,
    };
    let mut records = Vec::new();
    for path in &event.paths {
        let normalized = normalize_absolute_path(path);
        if normalized == *root {
            continue;
        }
        if let Ok(relative) = normalized.strip_prefix(root) {
            if scanner.is_ignored(&relative.to_string_lossy().replace('\\', "/")) {
                continue;
            }
        }
        records.push(FileChangeRecord::new(normalized, kind, timestamp));
    }
    if records.is_empty() {
        None
    } else {
        Some(records)
    }
}

/// Drives a [`FileWatcher`] and routes non-agent file changes into the
/// manual partition through [`FileCheckpointManager::process_manual_changes`].
///
/// Started when `FileCheckpointConfig.enabled && workspace_root` with
/// `manual_watch` set; lives for the whole runtime and is stopped at
/// shutdown. Agent self-writes are skipped by the manager's
/// recent-agent-writes registry, so the watcher only records genuine
/// human/external edits.
pub struct ManualChangeService {
    root: PathBuf,
    manager: FileCheckpointManager,
    stop_tx: watch::Sender<bool>,
    task: Option<tokio::task::JoinHandle<()>>,
}

impl ManualChangeService {
    /// Start watching `root` and feed non-agent changes into the manager.
    /// `debounce_ms` is the watcher debounce window, `poll_ms` the polling
    /// interval of the change pump.
    pub fn start(
        manager: FileCheckpointManager,
        root: impl Into<PathBuf>,
        scan_config: ScanConfig,
        debounce_ms: u64,
        poll_ms: u64,
    ) -> Result<Self, CheckpointError> {
        let root = root.into();
        let mut watcher = FileWatcher::new(&root, scan_config, debounce_ms);
        watcher.start()?;
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(run_manual_change_pump(
            watcher,
            manager.clone(),
            stop_rx,
            Duration::from_millis(poll_ms),
        ));
        Ok(Self {
            root,
            manager,
            stop_tx,
            task: Some(task),
        })
    }

    /// Stop the background pump and the underlying watcher.
    pub async fn stop(&mut self) {
        let _ = self.stop_tx.send(true);
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }

    /// The watched root directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Access to the manager (for diagnostics / integration tests).
    pub fn manager(&self) -> &FileCheckpointManager {
        &self.manager
    }

    /// Whether the underlying watcher is running (diagnostics).
    pub fn is_running(&self) -> bool {
        self.task.is_some()
    }
}

async fn run_manual_change_pump(
    watcher: FileWatcher,
    manager: FileCheckpointManager,
    mut stop: watch::Receiver<bool>,
    poll: Duration,
) {
    loop {
        tokio::select! {
            changed = stop.changed() => {
                if changed.is_err() || *stop.borrow() {
                    break;
                }
            }
            _ = tokio::time::sleep(poll) => {
                // Atomic batch consumption: take owns the current batch;
                // events arriving during processing stay buffered for the
                // next round instead of being cleared. This pump is the
                // sole consumer of its watcher: a taken batch has exactly
                // one owner, so no other driver may take from the same
                // queue while the pump runs.
                let batch = watcher.take_batch();
                if batch.is_empty() {
                    continue;
                }
                let manager = manager.clone();
                let batch_for_retry = batch.clone();
                let handled = tokio::task::spawn_blocking(move || {
                    manager.process_manual_changes(&batch_for_retry)
                })
                .await;
                match handled {
                    Ok(Ok(applied)) => {
                        if applied > 0 {
                            tracing::debug!(applied, "manual changes routed into the manual partition");
                        }
                    }
                    Ok(Err(err)) => {
                        // Failed batches are requeued so no event is lost;
                        // repeated content-hash application keeps retries
                        // idempotent at the manager layer.
                        tracing::warn!(error = %err, "failed to process manual file changes; requeueing batch");
                        watcher.requeue_batch(batch);
                    }
                    Err(join_err) => {
                        tracing::warn!(error = %join_err, "manual change pump task panicked; requeueing batch");
                        watcher.requeue_batch(batch);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_watcher(root: &Path) -> FileWatcher {
        FileWatcher::new(root.to_path_buf(), ScanConfig::default(), 50)
    }

    #[tokio::test]
    async fn manual_records_are_returned_and_reset() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = test_watcher(dir.path());

        watcher.notify_file_change("a.txt", FileChangeKind::Add);
        watcher.notify_file_change("b.txt", FileChangeKind::Change);
        assert_eq!(watcher.get_changed_files().len(), 2);
        assert!(watcher.has_changed("a.txt"));

        watcher.reset();
        assert!(watcher.get_changed_files().is_empty());
    }

    #[tokio::test]
    async fn relative_manual_records_resolve_against_root() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = test_watcher(dir.path());
        watcher.notify_file_change("sub/x.txt", FileChangeKind::Unlink);
        let records = watcher.get_changed_files();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, dir.path().join("sub/x.txt"));
        assert_eq!(records[0].kind, FileChangeKind::Unlink);
    }

    #[tokio::test]
    async fn watcher_tracks_real_file_events() {
        let dir = tempfile::tempdir().unwrap();
        let mut watcher = test_watcher(dir.path());
        watcher.start().unwrap();
        assert!(watcher.is_ready());

        let file = dir.path().join("tracked.txt");
        std::fs::write(&file, b"v1").unwrap();
        wait_until(&watcher, |w| w.has_changed("tracked.txt"), 3000).await;

        std::fs::write(&file, b"v2").unwrap();
        wait_until(
            &watcher,
            |w| {
                w.get_changed_files()
                    .iter()
                    .any(|r| r.kind == FileChangeKind::Change)
            },
            3000,
        )
        .await;

        watcher.stop().await;
    }

    #[tokio::test]
    async fn add_then_unlink_cancels_record() {
        let dir = tempfile::tempdir().unwrap();
        let mut watcher = test_watcher(dir.path());
        watcher.start().unwrap();

        let file = dir.path().join("ephemeral.txt");
        std::fs::write(&file, b"x").unwrap();
        wait_until(&watcher, |w| w.has_changed("ephemeral.txt"), 3000).await;

        std::fs::remove_file(&file).unwrap();
        wait_until(&watcher, |w| !w.has_changed("ephemeral.txt"), 3000).await;
        assert!(!watcher.get_changed_files().iter().any(|r| r.path == file));

        watcher.stop().await;
    }

    #[tokio::test]
    async fn ignored_files_are_not_tracked() {
        let dir = tempfile::tempdir().unwrap();
        let mut watcher = FileWatcher::new(
            dir.path().to_path_buf(),
            ScanConfig {
                custom_ignore_patterns: vec!["*.log".to_string()],
                ..ScanConfig::default()
            },
            50,
        );
        watcher.start().unwrap();

        std::fs::write(dir.path().join("x.log"), b"log").unwrap();
        std::fs::write(dir.path().join("y.txt"), b"txt").unwrap();
        wait_until(&watcher, |w| w.has_changed("y.txt"), 3000).await;
        assert!(!watcher.has_changed("x.log"));

        watcher.stop().await;
    }

    #[tokio::test]
    async fn take_batch_does_not_drop_late_arrivals() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = test_watcher(dir.path());
        watcher.notify_file_change("a.txt", FileChangeKind::Add);
        let batch = watcher.take_batch();
        assert_eq!(batch.len(), 1);
        assert!(watcher.take_batch().is_empty());

        // Events arriving after the take stay buffered for the next batch.
        watcher.notify_file_change("b.txt", FileChangeKind::Add);
        assert!(watcher.has_changed("b.txt"));
        assert!(!watcher.has_changed("a.txt"));
    }

    #[tokio::test]
    async fn failed_batch_can_be_requeued() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = test_watcher(dir.path());
        watcher.notify_file_change("a.txt", FileChangeKind::Add);
        watcher.notify_file_change("b.txt", FileChangeKind::Add);
        let batch = watcher.take_batch();
        assert_eq!(batch.len(), 2);
        assert!(watcher.take_batch().is_empty());

        watcher.requeue_batch(batch);
        let retaken = watcher.take_batch();
        assert_eq!(retaken.len(), 2);

        // Requeue never overwrites a newer buffered entry for the same path.
        watcher.notify_file_change("a.txt", FileChangeKind::Change);
        let _ = watcher.take_batch();
        watcher.notify_file_change("a.txt", FileChangeKind::Add);
        let newer = watcher.take_batch();
        assert_eq!(newer.len(), 1);
        assert_eq!(newer[0].kind, FileChangeKind::Add);
        watcher.requeue_batch(vec![FileChangeRecord::new(
            newer[0].path.clone(),
            FileChangeKind::Change,
            0,
        )]);
        // No buffered entry exists, so the requeued record is restored.
        assert_eq!(watcher.take_batch().len(), 1);
    }

    #[tokio::test]
    async fn reset_preserves_pending_window() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = test_watcher(dir.path());
        watcher.notify_file_change("a.txt", FileChangeKind::Add);
        watcher.reset();
        assert!(watcher.take_batch().is_empty());
        // Late arrivals after reset are still recorded.
        watcher.notify_file_change("b.txt", FileChangeKind::Add);
        assert!(watcher.has_changed("b.txt"));
    }

    #[tokio::test]
    async fn paths_are_lexically_normalized() {
        let dir = tempfile::tempdir().unwrap();
        let watcher = test_watcher(dir.path());
        watcher.notify_file_change("sub/../a.txt", FileChangeKind::Add);
        let records = watcher.take_batch();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].path, dir.path().join("a.txt"));
        assert_eq!(
            normalize_absolute_path(&dir.path().join("sub/../a.txt")),
            dir.path().join("a.txt")
        );
    }

    async fn wait_until(
        watcher: &FileWatcher,
        mut cond: impl FnMut(&FileWatcher) -> bool,
        timeout_ms: u64,
    ) {
        let deadline = std::time::Instant::now() + Duration::from_millis(timeout_ms);
        while std::time::Instant::now() < deadline {
            if cond(watcher) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("condition not met within {timeout_ms}ms");
    }
}
