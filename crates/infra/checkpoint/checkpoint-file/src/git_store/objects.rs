use crate::git_store::commit::{now_millis, signature_for};
use crate::git_store::{
    GitCommit, GitStore, GitStoreError, MAX_COMMIT_CAS_RETRIES, MODE_EXEC, MODE_FILE,
    SYSTEM_COMMITTER,
};
use gix_object::{Find as _, Write as _};
use std::collections::{HashMap, HashSet};

/// One tree entry: mode, name and object id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TreeEntry {
    pub mode: String,
    pub name: String,
    pub id: String,
}

/// Whether a string is a well-formed object id (40 hex chars).
pub fn is_hex_id(id: &str) -> bool {
    id.len() == 40 && gix_hash::ObjectId::from_hex(id.as_bytes()).is_ok()
}

pub(super) fn parse_object_id(id: &str) -> Result<gix_hash::ObjectId, GitStoreError> {
    if id.len() != 40 {
        return Err(GitStoreError::InvalidInput(format!("bad object id '{id}'")));
    }
    gix_hash::ObjectId::from_hex(id.as_bytes())
        .map_err(|e| GitStoreError::InvalidInput(format!("bad object id '{id}': {e}")))
}

impl GitStore {
    // ── objects (Gitoxide loose store: compression and identity handled there) ──

    fn odb(&self) -> Result<gix_odb::Handle, GitStoreError> {
        gix_odb::at(self.git_dir.join("objects"), gix_hash::Kind::Sha1)
            .map_err(|e| GitStoreError::Io(e.to_string()))
    }

    fn find_data(&self, id: &str) -> Result<(gix_object::Kind, Vec<u8>), GitStoreError> {
        let oid = parse_object_id(id)?;
        let odb = self.odb()?;
        let mut buf = Vec::new();
        let data = odb
            .try_find(&oid, &mut buf)
            .map_err(|e| GitStoreError::Corrupt {
                id: id.to_string(),
                reason: e.to_string(),
            })?
            .ok_or_else(|| GitStoreError::ObjectNotFound(id.to_string()))?;
        Ok((data.kind, data.data.to_vec()))
    }

    /// Store bytes as a blob object. Returns the object id.
    pub fn write_blob(&self, bytes: &[u8]) -> Result<String, GitStoreError> {
        let oid = self
            .odb()?
            .write_buf(gix_object::Kind::Blob, bytes)
            .map_err(|e| GitStoreError::Corrupt {
                id: "<new-blob>".to_string(),
                reason: e.to_string(),
            })?;
        Ok(oid.to_hex().to_string())
    }

    /// Load a blob's bytes.
    pub fn read_blob(&self, id: &str) -> Result<Vec<u8>, GitStoreError> {
        let (kind, body) = self.find_data(id)?;
        if kind != gix_object::Kind::Blob {
            return Err(GitStoreError::Corrupt {
                id: id.to_string(),
                reason: format!("expected blob, found {kind}"),
            });
        }
        Ok(body)
    }

    /// Store a sorted tree object from `(mode, name, id)` entries.
    pub fn write_tree(&self, entries: &[TreeEntry]) -> Result<String, GitStoreError> {
        let mut sorted = entries.to_vec();
        sorted.sort_by(|a, b| a.name.cmp(&b.name));
        let mut tree_entries = Vec::with_capacity(sorted.len());
        for entry in &sorted {
            let kind = match entry.mode.as_str() {
                "40000" => gix_object::tree::EntryKind::Tree,
                MODE_EXEC => gix_object::tree::EntryKind::BlobExecutable,
                MODE_FILE => gix_object::tree::EntryKind::Blob,
                other => {
                    return Err(GitStoreError::InvalidInput(format!(
                        "unsupported tree entry mode '{other}'"
                    )));
                }
            };
            let oid = parse_object_id(&entry.id)?;
            tree_entries.push(gix_object::tree::Entry {
                mode: kind.into(),
                filename: entry.name.as_bytes().to_vec().into(),
                oid,
            });
        }
        let tree = gix_object::Tree {
            entries: tree_entries,
        };
        let oid = self
            .odb()?
            .write(&tree)
            .map_err(|e| GitStoreError::Corrupt {
                id: "<new-tree>".to_string(),
                reason: e.to_string(),
            })?;
        Ok(oid.to_hex().to_string())
    }

    /// Parse a tree object into entries.
    pub fn read_tree(&self, id: &str) -> Result<Vec<TreeEntry>, GitStoreError> {
        let (kind, body) = self.find_data(id)?;
        if kind != gix_object::Kind::Tree {
            return Err(GitStoreError::Corrupt {
                id: id.to_string(),
                reason: format!("expected tree, found {kind}"),
            });
        }
        let tree = gix_object::TreeRef::from_bytes(&body, gix_hash::Kind::Sha1).map_err(|e| {
            GitStoreError::Corrupt {
                id: id.to_string(),
                reason: e.to_string(),
            }
        })?;
        Ok(tree
            .entries
            .iter()
            .map(|e| TreeEntry {
                mode: format!("{:o}", e.mode),
                name: e.filename.to_string(),
                id: e.oid.to_hex().to_string(),
            })
            .collect())
    }

    /// Expand a tree recursively into `path -> (mode, blob id)`.
    pub fn tree_to_files(
        &self,
        tree_id: &str,
    ) -> Result<HashMap<String, (String, String)>, GitStoreError> {
        let mut out = HashMap::new();
        self.tree_to_files_into(tree_id, String::new(), &mut out)?;
        Ok(out)
    }

    fn tree_to_files_into(
        &self,
        tree_id: &str,
        prefix: String,
        out: &mut HashMap<String, (String, String)>,
    ) -> Result<(), GitStoreError> {
        for entry in self.read_tree(tree_id)? {
            let path = if prefix.is_empty() {
                entry.name.clone()
            } else {
                format!("{}/{}", prefix, entry.name)
            };
            if entry.mode == "40000" {
                self.tree_to_files_into(&entry.id, path, out)?;
            } else {
                out.insert(path, (entry.mode, entry.id));
            }
        }
        Ok(())
    }

    /// Load a tree recursively into `path -> bytes`.
    pub fn tree_to_bytes(&self, tree_id: &str) -> Result<HashMap<String, Vec<u8>>, GitStoreError> {
        let files = self.tree_to_files(tree_id)?;
        let mut out = HashMap::with_capacity(files.len());
        for (path, (_, blob)) in files {
            out.insert(path, self.read_blob(&blob)?);
        }
        Ok(out)
    }

    /// Build a new tree from a parent tree plus path changes
    /// (`None` blob = deletion). Pure object pipelining, no checkout.
    /// Only trees along changed paths are loaded and rewritten; unchanged
    /// subtrees are reused by identifier.
    pub fn build_tree_from_parent(
        &self,
        parent_tree: Option<&str>,
        changes: &HashMap<String, Option<(String, Vec<u8>)>>,
    ) -> Result<String, GitStoreError> {
        if changes.is_empty() {
            if let Some(tree) = parent_tree {
                return Ok(tree.to_string());
            }
            return self.write_tree(&[]);
        }
        let mut blob_changes: HashMap<String, Option<(String, String)>> =
            HashMap::with_capacity(changes.len());
        for (path, change) in changes {
            match change {
                Some((mode, bytes)) => {
                    let blob = self.write_blob(bytes)?;
                    blob_changes.insert(path.clone(), Some((mode.clone(), blob)));
                }
                None => {
                    blob_changes.insert(path.clone(), None);
                }
            }
        }
        let staged: HashMap<String, Option<(String, String)>> = blob_changes;
        match self.build_tree_level(parent_tree, &staged)? {
            Some(id) => Ok(id),
            None => self.write_tree(&[]),
        }
    }

    fn build_tree_level(
        &self,
        parent_tree: Option<&str>,
        staged: &HashMap<String, Option<(String, String)>>,
    ) -> Result<Option<String>, GitStoreError> {
        let mut blobs: HashMap<String, (String, String)> = HashMap::new();
        let mut trees: HashMap<String, String> = HashMap::new();
        if let Some(tree) = parent_tree {
            for entry in self.read_tree(tree)? {
                if entry.mode == "40000" {
                    trees.insert(entry.name, entry.id);
                } else {
                    blobs.insert(entry.name, (entry.mode, entry.id));
                }
            }
        }
        let mut deeper: HashMap<String, HashMap<String, Option<(String, String)>>> = HashMap::new();
        for (path, change) in staged {
            match path.split_once('/') {
                None => match change {
                    Some((mode, blob)) => {
                        blobs.insert(path.clone(), (mode.clone(), blob.clone()));
                        trees.remove(path);
                    }
                    None => {
                        blobs.remove(path);
                        trees.remove(path);
                    }
                },
                Some((head, rest)) => {
                    deeper
                        .entry(head.to_string())
                        .or_default()
                        .insert(rest.to_string(), change.clone());
                }
            }
        }
        for (dir, sub) in &deeper {
            if blobs.contains_key(dir) {
                let full = sub
                    .keys()
                    .next()
                    .map_or(dir.clone(), |r| format!("{dir}/{r}"));
                return Err(GitStoreError::InvalidInput(format!(
                    "path collides with a file: '{full}'"
                )));
            }
            let child_parent = trees.get(dir).map(String::as_str);
            match self.build_tree_level(child_parent, sub)? {
                Some(child_id) => {
                    trees.insert(dir.clone(), child_id);
                }
                None => {
                    trees.remove(dir);
                }
            }
        }
        if blobs.is_empty() && trees.is_empty() {
            return Ok(None);
        }
        let mut entries = Vec::with_capacity(blobs.len() + trees.len());
        for (name, id) in &trees {
            entries.push(TreeEntry {
                mode: "40000".to_string(),
                name: name.clone(),
                id: id.clone(),
            });
        }
        for (name, (mode, blob)) in &blobs {
            entries.push(TreeEntry {
                mode: mode.clone(),
                name: name.clone(),
                id: blob.clone(),
            });
        }
        Ok(Some(self.write_tree(&entries)?))
    }

    /// Store a commit object. Empty-change commits are the caller's
    /// responsibility to skip (compare trees first).
    pub fn write_commit(
        &self,
        tree: &str,
        parents: &[String],
        author: &str,
        committer: &str,
        timestamp_millis: i64,
        message: &str,
    ) -> Result<String, GitStoreError> {
        let tree_id = parse_object_id(tree)?;
        let mut parent_ids = Vec::with_capacity(parents.len());
        for parent in parents {
            parent_ids.push(parse_object_id(parent)?);
        }
        let secs = timestamp_millis.div_euclid(1000);
        let mut text = message.trim_end_matches('\n').to_string();
        text.push('\n');
        let commit = gix_object::Commit {
            tree: tree_id,
            parents: parent_ids.into_iter().collect(),
            author: signature_for(author, secs),
            committer: signature_for(committer, secs),
            encoding: None,
            message: text.as_bytes().to_vec().into(),
            extra_headers: Vec::new(),
        };
        let oid = self
            .odb()?
            .write(&commit)
            .map_err(|e| GitStoreError::Corrupt {
                id: "<new-commit>".to_string(),
                reason: e.to_string(),
            })?;
        Ok(oid.to_hex().to_string())
    }

    /// Parse a commit object.
    pub fn read_commit(&self, id: &str) -> Result<GitCommit, GitStoreError> {
        let (kind, body) = self.find_data(id)?;
        if kind != gix_object::Kind::Commit {
            return Err(GitStoreError::Corrupt {
                id: id.to_string(),
                reason: format!("expected commit, found {kind}"),
            });
        }
        let commit =
            gix_object::CommitRef::from_bytes(&body, gix_hash::Kind::Sha1).map_err(|e| {
                GitStoreError::Corrupt {
                    id: id.to_string(),
                    reason: e.to_string(),
                }
            })?;
        let author_sig = commit.author().map_err(|e| GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        })?;
        let committer_sig = commit.committer().map_err(|e| GitStoreError::Corrupt {
            id: id.to_string(),
            reason: e.to_string(),
        })?;
        Ok(GitCommit {
            id: id.to_string(),
            tree: commit.tree().to_hex().to_string(),
            parents: commit.parents().map(|o| o.to_hex().to_string()).collect(),
            author: author_sig.name.to_string(),
            committer: committer_sig.name.to_string(),
            author_ts: author_sig.seconds().saturating_mul(1000),
            committer_ts: committer_sig.seconds().saturating_mul(1000),
            message: commit
                .message
                .to_string()
                .trim_end_matches('\n')
                .to_string(),
        })
    }

    /// Append a commit on a ref: build the tree from the ref head's tree
    /// plus `changes`, skip creating a commit when nothing changed (returns
    /// the existing head id), otherwise write the commit and move the ref.
    /// One call = one commit; multi-file operations pass all files at once
    /// so they stay atomic.
    ///
    /// The read-build-write sequence runs as a bounded compare-and-swap
    /// retry loop: a concurrent writer that moves the ref between our read
    /// and our write surfaces as a ref conflict, and we rebuild on the new
    /// head instead of silently orphaning the other writer's commit.
    pub fn commit_on_ref(
        &self,
        refname: &str,
        changes: &HashMap<String, Option<(String, Vec<u8>)>>,
        author: &str,
        message: &str,
    ) -> Result<CommitOnRefOutcome, GitStoreError> {
        for _ in 0..MAX_COMMIT_CAS_RETRIES {
            let head = self.read_ref(refname)?;
            let parent_tree = match &head {
                Some(id) => Some(self.read_commit(id)?.tree),
                None => None,
            };
            let tree = self.build_tree_from_parent(parent_tree.as_deref(), changes)?;
            if let Some(parent_tree) = parent_tree.as_deref() {
                if parent_tree == tree {
                    return Ok(CommitOnRefOutcome {
                        id: head.unwrap_or_default(),
                        created: false,
                    });
                }
            }
            let parents = head.clone().into_iter().collect::<Vec<_>>();
            let id = self.write_commit(
                &tree,
                &parents,
                author,
                SYSTEM_COMMITTER,
                now_millis(),
                message,
            )?;
            match self.compare_and_swap(refname, head.as_deref(), &id) {
                Ok(()) => return Ok(CommitOnRefOutcome { id, created: true }),
                Err(GitStoreError::RefConflict(_)) => continue,
                Err(other) => return Err(other),
            }
        }
        Err(GitStoreError::RefConflict(refname.to_string()))
    }

    /// Walk the commit graph from `start` in topological order, up to `limit`
    /// commits (0 = unlimited). Traversal order is preserved so clock skew
    /// never reorders history; timestamps are only payload data.
    pub fn log(&self, start: &str, limit: usize) -> Result<Vec<GitCommit>, GitStoreError> {
        let tip = parse_object_id(start)?;
        let odb = self.odb()?;
        let mut out = Vec::new();
        let walk = gix_traverse::commit::Simple::new([tip], odb);
        for next in walk {
            if limit > 0 && out.len() >= limit {
                break;
            }
            let Ok(info) = next else { continue };
            let id = info.id.to_hex().to_string();
            let Ok(commit) = self.read_commit(&id) else {
                continue;
            };
            out.push(commit);
        }
        Ok(out)
    }

    /// All commits reachable from any ref (used for cold index rebuilds).
    pub fn all_commits(&self) -> Result<Vec<GitCommit>, GitStoreError> {
        let refs = self.list_refs("refs/wf/")?;
        let mut tips = Vec::with_capacity(refs.len());
        for (_, id) in &refs {
            if let Ok(oid) = parse_object_id(id) {
                tips.push(oid);
            }
        }
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        if tips.is_empty() {
            return Ok(out);
        }
        let odb = self.odb()?;
        let walk = gix_traverse::commit::Simple::new(tips, odb);
        for next in walk {
            let Ok(info) = next else { continue };
            let id = info.id.to_hex().to_string();
            if !seen.insert(id.clone()) {
                continue;
            }
            let Ok(commit) = self.read_commit(&id) else {
                continue;
            };
            out.push(commit);
        }
        Ok(out)
    }

    /// Whether `ancestor` is reachable from `descendant`.
    pub fn is_ancestor(&self, ancestor: &str, descendant: &str) -> Result<bool, GitStoreError> {
        let ancestor_oid = parse_object_id(ancestor)?;
        let descendant_oid = parse_object_id(descendant)?;
        if ancestor_oid == descendant_oid {
            return Ok(true);
        }
        let odb = self.odb()?;
        let walk = gix_traverse::commit::Simple::new([descendant_oid], odb);
        for next in walk {
            let Ok(info) = next else { continue };
            if info.id == ancestor_oid {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Best common ancestor of two commits, if any.
    pub fn merge_base(&self, a: &str, b: &str) -> Result<Option<String>, GitStoreError> {
        let a_oid = parse_object_id(a)?;
        let b_oid = parse_object_id(b)?;
        if a_oid == b_oid {
            return Ok(Some(a.to_string()));
        }
        let mut ancestors = HashSet::new();
        {
            let odb = self.odb()?;
            let walk = gix_traverse::commit::Simple::new([a_oid], odb);
            for next in walk {
                let Ok(info) = next else { continue };
                ancestors.insert(info.id);
            }
        }
        {
            let odb = self.odb()?;
            let walk = gix_traverse::commit::Simple::new([b_oid], odb);
            for next in walk {
                let Ok(info) = next else { continue };
                if ancestors.contains(&info.id) {
                    return Ok(Some(info.id.to_hex().to_string()));
                }
            }
        }
        Ok(None)
    }
}

/// Result of [`GitStore::commit_on_ref`].
pub struct CommitOnRefOutcome {
    pub id: String,
    pub created: bool,
}
