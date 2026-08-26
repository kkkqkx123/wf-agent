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
}

/// A single file change record with an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileChangeRecord {
    pub path: PathBuf,
    pub kind: FileChangeKind,
    pub timestamp: i64,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

#[derive(Default)]
struct WatcherState {
    /// Records flushed from `pending`, since the last `reset()`.
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

    /// All changed files since the last `reset()`, with absolute paths.
    pub fn get_changed_files(&self) -> Vec<FileChangeRecord> {
        self.state
            .lock()
            .expect("watcher state poisoned")
            .changed
            .values()
            .cloned()
            .collect()
    }

    /// Changed file paths (absolute) since the last `reset()`.
    pub fn get_changed_paths(&self) -> Vec<PathBuf> {
        self.state
            .lock()
            .expect("watcher state poisoned")
            .changed
            .keys()
            .cloned()
            .collect()
    }

    /// Whether a file has changed since the last `reset()`. Relative paths
    /// are resolved against the watched root.
    pub fn has_changed(&self, file_path: impl AsRef<Path>) -> bool {
        let absolute = self.resolve_absolute(file_path.as_ref());
        self.state
            .lock()
            .expect("watcher state poisoned")
            .changed
            .contains_key(&absolute)
    }

    /// Clear tracked changes (call after a checkpoint is created).
    pub fn reset(&self) {
        let mut state = self.state.lock().expect("watcher state poisoned");
        state.changed.clear();
        state.pending.clear();
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
            FileChangeRecord {
                path: absolute,
                kind,
                timestamp: now_millis(),
            },
        );
    }

    fn resolve_absolute(&self, path: &Path) -> PathBuf {
        if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.root.join(path)
        }
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
    let kind = match event.kind {
        EventKind::Create(_) => FileChangeKind::Add,
        EventKind::Modify(_) => FileChangeKind::Change,
        EventKind::Remove(_) => FileChangeKind::Unlink,
        _ => return None,
    };
    let timestamp = now_millis();
    let mut records = Vec::new();
    for path in &event.paths {
        if path == root {
            continue;
        }
        if let Ok(relative) = path.strip_prefix(root) {
            if scanner.is_ignored(&relative.to_string_lossy().replace('\\', "/")) {
                continue;
            }
        }
        records.push(FileChangeRecord {
            path: path.clone(),
            kind,
            timestamp,
        });
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
                let records = watcher.get_changed_files();
                if records.is_empty() {
                    continue;
                }
                watcher.reset();
                let manager = manager.clone();
                let handled = tokio::task::spawn_blocking(move || {
                    manager.process_manual_changes(&records)
                })
                .await;
                match handled {
                    Ok(Ok(applied)) => {
                        if applied > 0 {
                            tracing::debug!(applied, "manual changes routed into the manual partition");
                        }
                    }
                    Ok(Err(err)) => {
                        tracing::warn!(error = %err, "failed to process manual file changes");
                    }
                    Err(join_err) => {
                        tracing::warn!(error = %join_err, "manual change pump task panicked");
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
