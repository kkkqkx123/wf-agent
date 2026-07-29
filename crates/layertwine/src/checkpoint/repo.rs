use crate::checkpoint::branch::Branch;
use crate::checkpoint::dag::CheckpointDag;
use crate::checkpoint::time_index::TimeIndex;
use crate::checkpoint::types::{Checkpoint, CheckpointMetadata};
use crate::core::snapshot::Snapshot;
use crate::core::types::{CheckpointId, ContentId, SnapshotId};
use crate::error::{LayertwineError, Result};
use crate::storage::repository::CheckpointPersist;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::RwLock;

/// Lazy-loading checkpoint map.
///
/// Loads checkpoints on-demand from the storage backend when they are
/// first accessed. This avoids loading the entire checkpoint history
/// into memory for repositories with large histories.
#[allow(dead_code)]
pub(crate) struct LazyCheckpointMap {
    loaded: RwLock<HashMap<CheckpointId, Checkpoint>>,
    storage: Option<Box<dyn CheckpointPersist>>,
}

#[allow(dead_code)]
impl LazyCheckpointMap {
    /// Create a new lazy checkpoint map with optional storage backend.
    pub fn new(storage: Option<Box<dyn CheckpointPersist>>) -> Self {
        LazyCheckpointMap {
            loaded: RwLock::new(HashMap::new()),
            storage,
        }
    }

    /// Create a new lazy checkpoint map with pre-loaded checkpoints.
    pub fn with_loaded(
        loaded: HashMap<CheckpointId, Checkpoint>,
        storage: Option<Box<dyn CheckpointPersist>>,
    ) -> Self {
        LazyCheckpointMap {
            loaded: RwLock::new(loaded),
            storage,
        }
    }

    /// Get a checkpoint by ID. If not loaded, attempt to load from storage.
    pub fn get(&self, id: &CheckpointId) -> Result<Checkpoint> {
        // Fast path: check if already loaded
        if let Some(cp) = self.loaded.read().map_err(|e| {
            LayertwineError::General(format!("RwLock poisoned: {}", e))
        })?.get(id) {
            return Ok(cp.clone());
        }

        // Slow path: load from storage
        if let Some(storage) = &self.storage {
            let cp = storage.get_checkpoint(id)?;
            self.loaded.write().map_err(|e| {
                LayertwineError::General(format!("RwLock poisoned: {}", e))
            })?.insert(*id, cp.clone());
            return Ok(cp);
        }

        Err(LayertwineError::NotFound(format!(
            "checkpoint {} not found",
            id
        )))
    }

    /// Check if a checkpoint ID is loaded.
    pub fn contains_key(&self, id: &CheckpointId) -> bool {
        self.loaded.read().map(|m| m.contains_key(id)).unwrap_or(false)
    }

    /// Insert a checkpoint into the map.
    pub fn insert(&self, id: CheckpointId, cp: Checkpoint) {
        if let Ok(mut map) = self.loaded.write() {
            map.insert(id, cp);
        }
    }

    /// Get all loaded checkpoint IDs.
    pub fn keys(&self) -> Vec<CheckpointId> {
        self.loaded.read().map(|m| m.keys().copied().collect()).unwrap_or_default()
    }

    /// Get a reference to the underlying loaded map (for migration purposes).
    pub fn into_loaded(self) -> HashMap<CheckpointId, Checkpoint> {
        self.loaded.into_inner().unwrap_or_default()
    }
}

/// Checkpoint Repository - Versioning Core
///
/// Git-independent versioning, managing checkpoint commits, branches, and DAG history.
/// When `storage` is set, all mutations auto-persist to the backend.
///
/// DAG is built dynamically from Checkpoint relationships and is not persisted.
pub struct CheckpointRepo {
    /// All branches
    pub branches: Vec<Branch>,
    /// Current branch index
    pub current_branch: usize,
    /// Checkpoint DAG (built dynamically from checkpoints)
    pub checkpoint_dag: CheckpointDag,
    /// All checkpoints (ID → Checkpoint)
    pub(crate) checkpoints: HashMap<CheckpointId, Checkpoint>,
    /// All snapshots (ID → Snapshot) — in-memory cache
    pub(crate) snapshots: HashMap<SnapshotId, Snapshot>,
    /// Optional persistence backend — when set, mutations auto-persist
    pub(crate) storage: Option<Box<dyn CheckpointPersist>>,
    /// Time index for fast time-based checkpoint queries
    pub time_index: TimeIndex,
    /// Checkpoints deleted since last sync — cleaned from storage in sync_all()
    deleted_checkpoints: HashSet<CheckpointId>,
    /// Branches deleted since last sync — cleaned from storage in sync_all()
    deleted_branches: HashSet<String>,
    /// Checkpoints modified in memory since last sync — only these are persisted in sync_all()
    pub(crate) dirty_checkpoints: HashSet<CheckpointId>,
}

impl CheckpointRepo {
    /// Create the root checkpoint and main branch.
    ///
    /// Shared by both [`new`] and [`load`] to avoid code duplication.
    fn create_root_checkpoint(
        initial_snapshots: Vec<SnapshotId>,
    ) -> (CheckpointId, Checkpoint, Branch) {
        let metadata = CheckpointMetadata::new("system", "root checkpoint");
        let root = Checkpoint::new(initial_snapshots, vec![], metadata);
        let root_id = root.id;
        let main_branch = Branch::new("main", root_id);
        (root_id, root, main_branch)
    }

    /// Create a new checkpoint repository with multi-file initialization support
    pub fn new(initial_snapshots: Vec<SnapshotId>) -> Self {
        let (root_id, root, main_branch) = Self::create_root_checkpoint(initial_snapshots);

        let mut dag = CheckpointDag::new();
        dag.add_node(root_id);

        let mut checkpoints = HashMap::new();
        let mut time_index = TimeIndex::new();
        time_index.insert(&root);
        checkpoints.insert(root_id, root);

        CheckpointRepo {
            branches: vec![main_branch],
            current_branch: 0,
            checkpoint_dag: dag,
            checkpoints,
            snapshots: HashMap::new(),
            storage: None,
            time_index,
            deleted_checkpoints: HashSet::new(),
            deleted_branches: HashSet::new(),
            dirty_checkpoints: HashSet::new(),
        }
    }

    /// Convenient single-snapshot compatible construction
    pub fn new_single(initial_snapshot: SnapshotId) -> Self {
        CheckpointRepo::new(vec![initial_snapshot])
    }

    /// Load checkpoint repository from persistent storage.
    ///
    /// If the database is empty, a root checkpoint + "main" branch are created automatically.
    /// All subsequent mutations will auto-persist through the provided storage backend.
    ///
    /// DAG is built dynamically from checkpoint relationships, not loaded from storage.
    pub fn load(storage: Box<dyn CheckpointPersist>) -> Result<Self> {
        let mut checkpoints: HashMap<CheckpointId, Checkpoint> = storage
            .list_checkpoints()?
            .into_iter()
            .map(|cp| (cp.id, cp))
            .collect();
        let mut branches = storage.list_branches()?;

        // Initialize with root when storage is empty
        if checkpoints.is_empty() {
            let (_root_id, root, main_branch) =
                Self::create_root_checkpoint(vec![]);

            storage.store_checkpoint(&root)?;
            checkpoints.insert(root.id, root);

            storage.store_branch(&main_branch)?;
            branches.push(main_branch);

            storage.store_metadata("current_branch", "main")?;
        }

        // Try to load DAG from persistent storage; fallback to rebuilding from checkpoints.
        // The DAG persistence enables fast loading without scanning all checkpoint entities.
        let checkpoint_dag = if storage.dag_has_node(&branches.first()
            .map(|b| b.head)
            .unwrap_or(ContentId([0u8; 32])))
            .unwrap_or(false)
        {
            // DAG table has data — load from persistent storage
            let (nodes, generation) = storage.load_dag()?;
            let mut dag = CheckpointDag::new();
            for (node_id, children) in &nodes {
                dag.add_node(*node_id);
                for child_id in children {
                    dag.add_edge_unchecked(*node_id, *child_id);
                }
            }
            // Set generation numbers for nodes known from the DAG table
            for (node_id, gen) in &generation {
                if dag.has_node(node_id) {
                    dag.set_generation(*node_id, *gen);
                }
            }
            dag
        } else {
            // Fallback: rebuild DAG from checkpoint relationships (backward compatibility).
            // This path is taken when the database was created before DAG persistence was added.
            Self::build_dag_from_checkpoints(&checkpoints)
        };

        // Build time index from all checkpoints
        let time_index =
            TimeIndex::from_checkpoints(&checkpoints.values().cloned().collect::<Vec<_>>());

        // Load snapshots referenced by all checkpoints from storage
        let mut snapshots = HashMap::new();
        let mut seen_snap_ids = HashSet::new();
        for cp in checkpoints.values() {
            for snap_id in &cp.baseline_snapshots {
                if seen_snap_ids.insert(*snap_id) {
                    if let Ok(snap) = storage.get_snapshot(snap_id) {
                        snapshots.insert(*snap_id, snap);
                    }
                }
            }
        }

        // Resolve current branch
        let current_branch_name = storage
            .load_metadata("current_branch")?
            .unwrap_or_else(|| "main".to_string());
        let current_branch = branches
            .iter()
            .position(|b| b.name == current_branch_name)
            .unwrap_or(0);

        Ok(CheckpointRepo {
            branches,
            current_branch,
            checkpoint_dag,
            checkpoints,
            snapshots,
            storage: Some(storage),
            time_index,
            deleted_checkpoints: HashSet::new(),
            deleted_branches: HashSet::new(),
            dirty_checkpoints: HashSet::new(),
        })
    }

    /// Attach a persistence backend to an existing in-memory repo.
    /// Subsequent mutations will auto-persist.
    pub fn attach_storage(&mut self, storage: Box<dyn CheckpointPersist>) {
        // All in-memory checkpoints need to be persisted to the new backend
        self.dirty_checkpoints = self.checkpoints.keys().copied().collect();
        self.storage = Some(storage);
    }

    /// Build DAG dynamically from checkpoint relationships.
    fn build_dag_from_checkpoints(
        checkpoints: &HashMap<CheckpointId, Checkpoint>,
    ) -> CheckpointDag {
        // Use unchecked edge insertion: checkpoint parent relationships stored in
        // SQLite are inherently acyclic, so cycle detection (BFS) is unnecessary.
        let mut dag = CheckpointDag::new();
        for (id, cp) in checkpoints {
            dag.add_node(*id);
            for parent in &cp.parents {
                dag.add_edge_unchecked(*parent, *id);
            }
        }
        dag
    }

    /// Rebuild the DAG from storage (or in-memory checkpoints).
    ///
    /// Call this after checkpoints have been modified directly through the storage
    /// backend (e.g., via `commit_staged_to_checkpoint` in the staged layer API)
    /// to keep the DAG in sync with persisted data.
    ///
    /// When a storage backend is attached, reloads all checkpoints from storage
    /// and rebuilds the DAG from scratch. When no storage backend is available,
    /// rebuilds from the in-memory checkpoint map.
    pub fn rebuild_dag(&mut self) -> Result<()> {
        if let Some(storage) = &self.storage {
            let checkpoints: HashMap<CheckpointId, Checkpoint> = storage
                .list_checkpoints()?
                .into_iter()
                .map(|cp| (cp.id, cp))
                .collect();
            self.checkpoints = checkpoints;
        }
        self.checkpoint_dag = Self::build_dag_from_checkpoints(&self.checkpoints);
        self.time_index = TimeIndex::from_checkpoints(
            &self.checkpoints.values().cloned().collect::<Vec<_>>(),
        );
        Ok(())
    }

    /// Persist a specific checkpoint to storage (used after in-memory metadata changes).
    pub fn sync_checkpoint(&self, cp_id: &CheckpointId) -> Result<()> {
        if let Some(storage) = &self.storage {
            if let Some(cp) = self.checkpoints.get(cp_id) {
                storage.store_checkpoint(cp)?;
            }
        }
        Ok(())
    }

    /// Persist the full current state (all dirty checkpoints, branches, current_branch metadata).
    ///
    /// Only checkpoints that were created or modified since the last sync are persisted.
    /// Also cleans up any checkpoints and branches that were deleted in memory
    /// from the storage backend.
    ///
    /// DAG edges are persisted alongside checkpoint data to ensure the DAG
    /// structure is recoverable from storage without scanning all checkpoints.
    pub fn sync_all(&mut self) -> Result<()> {
        if let Some(storage) = &self.storage {
            // Persist only dirty (modified since last sync) checkpoints
            for cp_id in self.dirty_checkpoints.drain() {
                if let Some(cp) = self.checkpoints.get(&cp_id) {
                    storage.store_checkpoint(cp)?;
                    // Persist DAG edges for each parent relationship
                    for parent_id in &cp.parents {
                        if let Some(gen) = self.checkpoint_dag.generation(&cp_id) {
                            storage.store_dag_edge(parent_id, &cp_id, gen)?;
                        }
                    }
                }
            }
            // Clean up checkpoint deletions
            for deleted_id in self.deleted_checkpoints.drain() {
                // Ignore "not found" errors — already deleted is fine
                let _ = storage.delete_checkpoint(&deleted_id);
                let _ = storage.delete_dag_node(&deleted_id);
            }
            // Clean up branch deletions
            for deleted_name in self.deleted_branches.drain() {
                let _ = storage.delete_branch(&deleted_name);
            }
            // Clean up branch deletions
            for deleted_name in self.deleted_branches.drain() {
                let _ = storage.delete_branch(&deleted_name);
            }
            for branch in &self.branches {
                storage.store_branch(branch)?;
            }
            storage.store_metadata("current_branch", &self.branches[self.current_branch].name)?;
        }
        Ok(())
    }

    // Checkpoint operations -

    /// Getting the specified checkpoints
    pub fn get_checkpoint(&self, id: &CheckpointId) -> Result<&Checkpoint> {
        self.checkpoints
            .get(id)
            .ok_or_else(|| LayertwineError::NotFound(format!("checkpoint {} not found", id)))
    }

    /// Getting variable references
    pub fn get_checkpoint_mut(&mut self, id: &CheckpointId) -> Result<&mut Checkpoint> {
        self.checkpoints
            .get_mut(id)
            .ok_or_else(|| LayertwineError::NotFound(format!("checkpoint {} not found", id)))
    }

    /// Commit: Packages the current state of staged as a Checkpoint.
    ///
    /// 1. Create a new Checkpoint (supports multi-file snapshots)
    /// 2. Add to DAG
    /// 3. Update branch head
    /// 4. Auto-persist to storage if backend is attached
    ///
    /// DAG is not persisted; it is maintained in memory and rebuilt when loading.
    pub fn commit(
        &mut self,
        snapshot_ids: Vec<SnapshotId>,
        message: &str,
        author: &str,
    ) -> Result<CheckpointId> {
        if snapshot_ids.is_empty() {
            return Err(LayertwineError::Checkpoint(
                "cannot commit with empty snapshot list".to_string(),
            ));
        }

        let current_head = self.current_branch_head();

        // Check if there are actual changes compared to current head
        let current_cp = self.checkpoints.get(&current_head).ok_or_else(|| {
            LayertwineError::NotFound(format!("checkpoint {} not found", current_head))
        })?;

        if current_cp.baseline_snapshots == snapshot_ids {
            return Err(LayertwineError::Checkpoint(
                "no changes to commit (same snapshot IDs as current head)".to_string(),
            ));
        }

        let metadata = CheckpointMetadata::new(author, message);
        let cp = Checkpoint::new(snapshot_ids, vec![current_head], metadata);
        let cp_id = cp.id;

        // Use &cp before moving it into the map (avoids redundant HashMap lookup)
        self.time_index.insert(&cp);
        self.checkpoint_dag.add_node(cp_id);
        self.checkpoint_dag.add_edge(current_head, cp_id);
        self.current_branch_mut().set_head(cp_id);

        // Auto-persist checkpoint + DAG edge
        if let Some(storage) = &self.storage {
            storage.store_checkpoint(&cp)?;
            if let Some(gen) = self.checkpoint_dag.generation(&cp_id) {
                storage.store_dag_edge(&current_head, &cp_id, gen)?;
            }
            let branch = &self.branches[self.current_branch];
            storage.update_branch_head(&branch.name, &cp_id)?;
        }

        self.dirty_checkpoints.insert(cp_id);
        self.checkpoints.insert(cp_id, cp);

        Ok(cp_id)
    }

    /// Compatible with single snapshot submission
    pub fn commit_single(
        &mut self,
        snapshot_id: SnapshotId,
        message: &str,
        author: &str,
    ) -> Result<CheckpointId> {
        self.commit(vec![snapshot_id], message, author)
    }

    /// Commit without the "no changes" guard.
    ///
    /// Unlike [`commit`], this allows creating a checkpoint with the same
    /// snapshot IDs as the current head (e.g. for tagging or metadata-only commits).
    /// Also allows empty snapshot lists for annotation-style commits.
    ///
    /// Returns the new checkpoint ID.
    pub fn commit_allow_empty(
        &mut self,
        snapshot_ids: Vec<SnapshotId>,
        message: &str,
        author: &str,
    ) -> Result<CheckpointId> {
        let current_head = self.current_branch_head();
        let metadata = CheckpointMetadata::new(author, message);
        let cp = Checkpoint::new(snapshot_ids, vec![current_head], metadata);
        let cp_id = cp.id;

        self.time_index.insert(&cp);
        self.checkpoint_dag.add_node(cp_id);
        self.checkpoint_dag.add_edge(current_head, cp_id);
        self.current_branch_mut().set_head(cp_id);

        if let Some(storage) = &self.storage {
            storage.store_checkpoint(&cp)?;
            if let Some(gen) = self.checkpoint_dag.generation(&cp_id) {
                storage.store_dag_edge(&current_head, &cp_id, gen)?;
            }
            let branch = &self.branches[self.current_branch];
            storage.update_branch_head(&branch.name, &cp_id)?;
        }

        self.dirty_checkpoints.insert(cp_id);
        self.checkpoints.insert(cp_id, cp);

        Ok(cp_id)
    }

    // Branching out.

    /// Get the head Checkpoint ID of the current branch
    pub fn current_branch_head(&self) -> CheckpointId {
        self.branches[self.current_branch].head
    }

    /// Get a mutable reference to the current branch
    pub fn current_branch_mut(&mut self) -> &mut Branch {
        &mut self.branches[self.current_branch]
    }

    /// Get current branch name
    pub fn current_branch_name(&self) -> &str {
        &self.branches[self.current_branch].name
    }

    /// Create a new branch based on the current head.
    /// Auto-persists the new branch to storage.
    pub fn create_branch(&mut self, name: &str) -> Result<()> {
        if self.branches.iter().any(|b| b.name == name) {
            return Err(LayertwineError::Checkpoint(format!(
                "branch '{}' already exists",
                name
            )));
        }
        let head = self.current_branch_head();
        let branch = Branch::new(name, head);
        if let Some(storage) = &self.storage {
            storage.store_branch(&branch)?;
        }
        self.branches.push(branch);
        Ok(())
    }

    /// Create a new branch on the specified checkpoint.
    /// Auto-persists the new branch to storage.
    pub fn create_branch_from(&mut self, name: &str, from_checkpoint: CheckpointId) -> Result<()> {
        if !self.checkpoints.contains_key(&from_checkpoint) {
            return Err(LayertwineError::NotFound(format!(
                "checkpoint {} not found",
                from_checkpoint
            )));
        }
        if self.branches.iter().any(|b| b.name == name) {
            return Err(LayertwineError::Checkpoint(format!(
                "branch '{}' already exists",
                name
            )));
        }
        let branch = Branch::new(name, from_checkpoint);
        if let Some(storage) = &self.storage {
            storage.store_branch(&branch)?;
        }
        self.branches.push(branch);
        Ok(())
    }

    /// Switching Branches
    /// Auto-persists the current branch name to storage.
    pub fn switch_branch(&mut self, name: &str) -> Result<CheckpointId> {
        let idx = self
            .branches
            .iter()
            .position(|b| b.name == name)
            .ok_or_else(|| LayertwineError::NotFound(format!("branch '{}' not found", name)))?;

        self.current_branch = idx;
        if let Some(storage) = &self.storage {
            storage.store_metadata("current_branch", name)?;
        }
        Ok(self.branches[idx].head)
    }

    /// List all branches
    pub fn list_branches(&self) -> &[Branch] {
        &self.branches
    }

    /// Find Branch Index
    pub fn find_branch(&self, name: &str) -> Result<usize> {
        self.branches
            .iter()
            .position(|b| b.name == name)
            .ok_or_else(|| LayertwineError::NotFound(format!("branch '{}' not found", name)))
    }

    /// Delete a branch by name.
    ///
    /// Removes it from memory and immediately deletes from storage when a backend is attached.
    /// When no storage backend is available, marks it for deferred cleanup in [`sync_all`].
    pub fn remove_branch(&mut self, name: &str) -> Result<()> {
        let idx = self.find_branch(name)?;
        self.branches.remove(idx);
        // Track for deferred sync only when no immediate storage is available
        if self.storage.is_none() {
            self.deleted_branches.insert(name.to_string());
        }
        if let Some(storage) = &self.storage {
            storage.delete_branch(name)?;
        }
        Ok(())
    }

    /// Get the head of the specified branch
    pub fn get_branch_head(&self, name: &str) -> Result<CheckpointId> {
        let idx = self.find_branch(name)?;
        Ok(self.branches[idx].head)
    }

    /// Merge branch: Merge source_branch into the current branch.
    ///
    /// Generate multiple parent Checkpoints to add to the DAG.
    /// Auto-persists the new checkpoint and updated branch head.
    ///
    /// DAG is not persisted; it is maintained in memory and rebuilt when loading.
    pub fn merge_branches(
        &mut self,
        source_branch: &str,
        snapshot_ids: Vec<SnapshotId>,
        message: &str,
        author: &str,
    ) -> Result<CheckpointId> {
        if snapshot_ids.is_empty() {
            return Err(LayertwineError::Checkpoint(
                "cannot merge with empty snapshot list".to_string(),
            ));
        }
        let source_head = self.get_branch_head(source_branch)?;
        let current_head = self.current_branch_head();

        let metadata = CheckpointMetadata::new(author, message);
        let cp = Checkpoint::new(snapshot_ids, vec![current_head, source_head], metadata);
        let cp_id = cp.id;

        // Use &cp before moving it into the map (avoids redundant HashMap lookup)
        self.time_index.insert(&cp);
        self.checkpoint_dag.add_node(cp_id);
        self.checkpoint_dag.add_edge(current_head, cp_id);
        self.checkpoint_dag.add_edge(source_head, cp_id);
        self.current_branch_mut().set_head(cp_id);

        // Auto-persist checkpoint + DAG edges (both parents)
        if let Some(storage) = &self.storage {
            storage.store_checkpoint(&cp)?;
            if let Some(gen) = self.checkpoint_dag.generation(&cp_id) {
                storage.store_dag_edge(&current_head, &cp_id, gen)?;
                storage.store_dag_edge(&source_head, &cp_id, gen)?;
            }
            let branch = &self.branches[self.current_branch];
            storage.update_branch_head(&branch.name, &cp_id)?;
        }

        self.dirty_checkpoints.insert(cp_id);
        self.checkpoints.insert(cp_id, cp);

        Ok(cp_id)
    }

    // - -Logs - -

    /// Trace back the ancestor chain from the current head
    ///
    /// Returns checkpoints in BFS order from head to root.
    /// For merge commits, traverses all parents to ensure no history is lost.
    pub fn log(&self, count: usize) -> Vec<&Checkpoint> {
        self.log_from(&self.current_branch_head(), count)
    }

    /// Backtracking from a given checkpoint
    ///
    /// Returns checkpoints in BFS order from start checkpoint.
    /// For merge commits, traverses all parents to ensure no history is lost.
    pub fn log_from(&self, start: &CheckpointId, count: usize) -> Vec<&Checkpoint> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(*start);

        while let Some(cp_id) = queue.pop_front() {
            if result.len() >= count {
                break;
            }
            if !visited.insert(cp_id) {
                continue;
            }
            if let Some(cp) = self.checkpoints.get(&cp_id) {
                result.push(cp);
                for parent in &cp.parents {
                    if !visited.contains(parent) {
                        queue.push_back(*parent);
                    }
                }
            }
        }

        result
    }

    // - - Enquiry - -

    /// Get the number of all checkpoints
    pub fn checkpoint_count(&self) -> usize {
        self.checkpoints.len()
    }

    /// DAG references
    pub fn dag(&self) -> &CheckpointDag {
        &self.checkpoint_dag
    }

    /// Delete checkpoints
    /// Auto-persists the deletion (DAG is not persisted as it is rebuilt from checkpoints).
    ///
    /// When a storage backend is attached, the deletion is written immediately.
    /// When no storage is attached, the deletion is tracked in `deleted_checkpoints`
    /// so that a subsequent [`sync_all`] (after [`attach_storage`]) can clean it up.
    pub fn remove_checkpoint(&mut self, id: &CheckpointId) -> Result<()> {
        if let Some(cp) = self.checkpoints.remove(id) {
            self.time_index.remove(&cp);
            // Track deletion for deferred sync only when no immediate storage is available
            if self.storage.is_none() {
                self.deleted_checkpoints.insert(*id);
            }
        } else {
            return Err(LayertwineError::NotFound(format!(
                "checkpoint {} not found",
                id
            )));
        }
        self.checkpoint_dag.remove_node(id);
        if let Some(storage) = &self.storage {
            storage.delete_checkpoint(id)?;
            storage.delete_dag_node(id)?;
        }
        Ok(())
    }

    /// Rollback to a previous checkpoint.
    ///
    /// Returns the baseline snapshots of the target checkpoint, which can be used
    /// to restore the state of all files at that checkpoint.
    ///
    /// Note: This does not modify the repository state; it only returns the snapshot IDs
    /// that should be used to restore the file partitions.
    pub fn rollback_to(&self, cp_id: &CheckpointId) -> Result<Vec<SnapshotId>> {
        let cp = self.get_checkpoint(cp_id)?;
        Ok(cp.baseline_snapshots.clone())
    }

    /// Get the ancestor chain from the current head back to the specified checkpoint.
    ///
    /// Returns checkpoints in BFS order from head to target (inclusive).
    /// Useful for undo/redo operations or understanding the path between two checkpoints.
    pub fn get_ancestors_to(&self, target: &CheckpointId) -> Result<Vec<&Checkpoint>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.current_branch_head());

        while let Some(cp_id) = queue.pop_front() {
            if !visited.insert(cp_id) {
                continue;
            }
            if let Some(cp) = self.checkpoints.get(&cp_id) {
                result.push(cp);
                if cp_id == *target {
                    break;
                }
                for parent in &cp.parents {
                    if !visited.contains(parent) {
                        queue.push_back(*parent);
                    }
                }
            }
        }

        if result.last().map(|cp| &cp.id) != Some(target) {
            return Err(LayertwineError::NotFound(format!(
                "checkpoint {} is not an ancestor of current head",
                target
            )));
        }

        Ok(result)
    }

    /// Get a snapshot by its ID from in-memory cache or storage backend
    pub fn get_snapshot_by_id(&self, snap_id: &SnapshotId) -> Result<Snapshot> {
        if let Some(snap) = self.snapshots.get(snap_id) {
            return Ok(snap.clone());
        }
        if let Some(storage) = &self.storage {
            return storage.get_snapshot(snap_id).map_err(LayertwineError::from);
        }
        Err(LayertwineError::NotFound(format!(
            "Snapshot {} not found",
            snap_id
        )))
    }

    /// Insert a snapshot into the in-memory cache.
    ///
    /// Snapshots must be cached (or stored in the storage backend) for
    /// restore operations to resolve their content.
    pub fn cache_snapshot(&mut self, snap: Snapshot) {
        self.snapshots.insert(snap.id, snap);
    }

    /// Register a snapshot source mapping in the given checkpoint.
    ///
    /// The source string identifies the origin of the snapshot
    /// (e.g. "file://src/main.rs", "agent://state").
    pub fn set_snapshot_source(
        &mut self,
        cp_id: &CheckpointId,
        snap_id: SnapshotId,
        source: String,
    ) -> Result<()> {
        let cp = self
            .checkpoints
            .get_mut(cp_id)
            .ok_or_else(|| LayertwineError::NotFound(format!("checkpoint {} not found", cp_id)))?;
        cp.snapshot_sources.insert(snap_id, source);
        self.dirty_checkpoints.insert(*cp_id);
        Ok(())
    }

    /// Get the full ancestry chain from root to the given checkpoint.
    ///
    /// Traverses parents via BFS and returns checkpoint IDs in topological
    /// order (root first, target last). Used by restore operations for
    /// delta reconstruction.
    pub fn get_ancestry_chain(&self, cp_id: &CheckpointId) -> Result<Vec<CheckpointId>> {
        let mut result = Vec::new();
        let mut visited = HashSet::new();
        let mut current = vec![*cp_id];
        let mut next = Vec::new();

        while !current.is_empty() {
            for cid in current.drain(..) {
                if !visited.insert(cid) {
                    continue;
                }
                result.push(cid);
                if let Ok(cp) = self.get_checkpoint(&cid) {
                    for parent in &cp.parents {
                        if !visited.contains(parent) {
                            next.push(*parent);
                        }
                    }
                }
            }
            std::mem::swap(&mut current, &mut next);
        }

        result.reverse();
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::ContentId;

    fn dummy_snapshot_id(n: u8) -> SnapshotId {
        ContentId::from_content(&[n; 8])
    }

    #[test]
    fn test_init_repo() {
        let snap = dummy_snapshot_id(1);
        let repo = CheckpointRepo::new_single(snap);
        assert_eq!(repo.branches.len(), 1);
        assert_eq!(repo.branches[0].name, "main");
        assert_eq!(repo.checkpoint_count(), 1);
    }

    #[test]
    fn test_init_repo_multi_snapshot() {
        let snap1 = dummy_snapshot_id(1);
        let snap2 = dummy_snapshot_id(2);
        let snapshots = vec![snap1, snap2];
        let repo = CheckpointRepo::new(snapshots);
        assert_eq!(repo.checkpoint_count(), 1);
        let root = repo.get_checkpoint(&repo.current_branch_head()).unwrap();
        assert_eq!(root.baseline_snapshots.len(), 2);
    }

    #[test]
    fn test_linear_commit() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        let snap2 = dummy_snapshot_id(2);
        let cp1 = repo.commit_single(snap2, "second commit", "user").unwrap();

        let snap3 = dummy_snapshot_id(3);
        let cp2 = repo.commit_single(snap3, "third commit", "user").unwrap();

        let log = repo.log(10);
        assert_eq!(log.len(), 3);
        assert_eq!(log[0].id, cp2);
        assert_eq!(log[1].id, cp1);
    }

    #[test]
    fn test_multi_snapshot_commit() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        let snap2 = dummy_snapshot_id(2);
        let snap3 = dummy_snapshot_id(3);
        let cp1 = repo
            .commit(vec![snap2, snap3], "multi-file commit", "user")
            .unwrap();

        let cp = repo.get_checkpoint(&cp1).unwrap();
        assert_eq!(cp.baseline_snapshots.len(), 2);
    }

    #[test]
    fn test_create_and_switch_branch() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);

        repo.create_branch("feature").unwrap();
        assert_eq!(repo.branches.len(), 2);

        repo.switch_branch("feature").unwrap();
        assert_eq!(repo.current_branch_name(), "feature");
    }

    #[test]
    fn test_merge_branches() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        let snap2 = dummy_snapshot_id(2);
        repo.commit_single(snap2, "main v2", "user").unwrap();

        repo.create_branch("feature").unwrap();

        let snap3 = dummy_snapshot_id(3);
        repo.commit_single(snap3, "feature v1", "user").unwrap();

        repo.switch_branch("main").unwrap();

        let snap4 = dummy_snapshot_id(4);
        let merge_cp = repo
            .merge_branches("feature", vec![snap4], "merge feature", "user")
            .unwrap();

        let cp = repo.get_checkpoint(&merge_cp).unwrap();
        assert_eq!(
            cp.parents.len(),
            2,
            "merge checkpoint should have 2 parents"
        );
    }

    #[test]
    fn test_log_count() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        for i in 2..=10 {
            repo.commit_single(dummy_snapshot_id(i), &format!("commit {}", i), "user")
                .unwrap();
        }

        let log = repo.log(5);
        assert_eq!(log.len(), 5);
        assert_eq!(log[0].metadata.message, "commit 10");
    }

    #[test]
    fn test_list_branches() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);

        repo.create_branch("feature-a").unwrap();
        repo.create_branch("feature-b").unwrap();

        let branches = repo.list_branches();
        assert_eq!(branches.len(), 3);
    }

    #[test]
    fn test_empty_commit_fails() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);
        let result = repo.commit(vec![], "empty", "user");
        assert!(result.is_err());
    }

    #[test]
    fn test_switch_nonexistent_branch_fails() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);
        let result = repo.switch_branch("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_delete_nonexistent_checkpoint_fails() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);
        let fake_id = CheckpointId::from_content(b"fake");
        let result = repo.remove_checkpoint(&fake_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_create_duplicate_branch_fails() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);
        repo.create_branch("feature").unwrap();
        let result = repo.create_branch("feature");
        assert!(result.is_err());
    }

    #[test]
    fn test_create_branch_from_nonexistent_checkpoint_fails() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);
        let fake_id = CheckpointId::from_content(b"fake");
        let result = repo.create_branch_from("feature", fake_id);
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_with_empty_snapshots_fails() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);
        repo.create_branch("feature").unwrap();
        let result = repo.merge_branches("feature", vec![], "merge", "user");
        assert!(result.is_err());
    }

    #[test]
    fn test_merge_nonexistent_branch_fails() {
        let snap1 = dummy_snapshot_id(1);
        let snap2 = dummy_snapshot_id(2);
        let mut repo = CheckpointRepo::new_single(snap1);
        let result = repo.merge_branches("nonexistent", vec![snap2], "merge", "user");
        assert!(result.is_err());
    }

    #[test]
    fn test_log_from_merge_commit() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        let snap2 = dummy_snapshot_id(2);
        repo.commit_single(snap2, "main v2", "user").unwrap();

        repo.create_branch("feature").unwrap();

        let snap3 = dummy_snapshot_id(3);
        repo.commit_single(snap3, "feature v1", "user").unwrap();

        repo.switch_branch("main").unwrap();

        let snap4 = dummy_snapshot_id(4);
        repo.merge_branches("feature", vec![snap4], "merge feature", "user")
            .unwrap();

        let log = repo.log(10);
        assert!(log.len() >= 4);

        let merge_cp = &log[0];
        assert_eq!(merge_cp.parents.len(), 2, "merge should have 2 parents");
    }

    #[test]
    fn test_get_branch_head_nonexistent_fails() {
        let snap = dummy_snapshot_id(1);
        let repo = CheckpointRepo::new_single(snap);
        let result = repo.get_branch_head("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_multiple_branch_switching() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);

        repo.create_branch("feature-a").unwrap();
        repo.create_branch("feature-b").unwrap();

        repo.switch_branch("feature-a").unwrap();
        assert_eq!(repo.current_branch_name(), "feature-a");

        repo.switch_branch("feature-b").unwrap();
        assert_eq!(repo.current_branch_name(), "feature-b");

        repo.switch_branch("main").unwrap();
        assert_eq!(repo.current_branch_name(), "main");
    }

    #[test]
    fn test_remove_checkpoint_updates_dag() {
        let snap1 = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap1);

        let snap2 = dummy_snapshot_id(2);
        let _cp1 = repo.commit_single(snap2, "second", "user").unwrap();

        let snap3 = dummy_snapshot_id(3);
        let cp2 = repo.commit_single(snap3, "third", "user").unwrap();

        repo.remove_checkpoint(&cp2).unwrap();

        assert_eq!(repo.checkpoint_count(), 2);
        assert!(repo.get_checkpoint(&cp2).is_err());
        assert!(!repo.checkpoint_dag.has_node(&cp2));
    }

    #[test]
    fn test_find_branch() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);

        repo.create_branch("feature").unwrap();
        let idx = repo.find_branch("feature").unwrap();
        assert_eq!(repo.branches[idx].name, "feature");

        let result = repo.find_branch("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_commit_allow_empty_same_snapshot() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);

        // Normal commit would fail with "no changes", but allow_empty should succeed
        let cp_id = repo.commit_allow_empty(vec![snap], "tag commit", "user").unwrap();

        let cp = repo.get_checkpoint(&cp_id).unwrap();
        assert_eq!(cp.baseline_snapshots, vec![snap]);
        assert_eq!(cp.metadata.message, "tag commit");
        assert_eq!(repo.checkpoint_count(), 2);
    }

    #[test]
    fn test_commit_allow_empty_zero_snapshots() {
        let snap = dummy_snapshot_id(1);
        let mut repo = CheckpointRepo::new_single(snap);

        // Allow committing with empty snapshot list (annotation-style)
        let cp_id = repo.commit_allow_empty(vec![], "annotation", "user").unwrap();

        let cp = repo.get_checkpoint(&cp_id).unwrap();
        assert!(cp.baseline_snapshots.is_empty());
        assert_eq!(cp.metadata.message, "annotation");
        assert_eq!(repo.checkpoint_count(), 2);
    }
}
