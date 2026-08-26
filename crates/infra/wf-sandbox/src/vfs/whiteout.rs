use std::collections::HashSet;
use std::path::PathBuf;

pub struct WhiteoutCache {
    entries: HashSet<PathBuf>,
}

impl Default for WhiteoutCache {
    fn default() -> Self {
        Self::new()
    }
}

impl WhiteoutCache {
    pub fn new() -> Self {
        Self {
            entries: HashSet::new(),
        }
    }

    pub fn whiteout(&mut self, path: PathBuf) {
        self.entries.insert(path);
    }

    pub fn is_whiteout(&self, path: &PathBuf) -> bool {
        self.entries.contains(path)
    }

    pub fn remove_whiteout(&mut self, path: &PathBuf) -> bool {
        self.entries.remove(path)
    }

    /// Snapshot of all whiteouted paths, for batch consumers such as
    /// committing deletions onto the base directory.
    pub fn paths(&self) -> Vec<PathBuf> {
        self.entries.iter().cloned().collect()
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }
}
