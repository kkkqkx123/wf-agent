use std::collections::HashMap;
use std::path::PathBuf;

pub type MemoryDelta = HashMap<PathBuf, Vec<u8>>;

pub fn delta_read(
    delta: &MemoryDelta,
    path: &PathBuf,
) -> Option<Vec<u8>> {
    delta.get(path).cloned()
}

pub fn delta_write(
    delta: &mut MemoryDelta,
    path: PathBuf,
    data: Vec<u8>,
) {
    delta.insert(path, data);
}

pub fn delta_remove(delta: &mut MemoryDelta, path: &PathBuf) -> bool {
    delta.remove(path).is_some()
}

pub fn delta_contains(delta: &MemoryDelta, path: &PathBuf) -> bool {
    delta.contains_key(path)
}
