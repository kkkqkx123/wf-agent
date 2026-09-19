use crate::core::file_node::FileNode;

#[test]
fn test_file_node_new() {
    let content = b"test content";
    let file = FileNode::new("test.txt".into(), content);

    assert_eq!(file.path_str(), "test.txt");
    assert_ne!(file.base_hash, [0u8; 32]);
}

#[test]
fn test_file_node_hash_consistency() {
    let content = b"same content";
    let file1 = FileNode::new("path1.txt".into(), content);
    let file2 = FileNode::new("path2.txt".into(), content);

    assert_eq!(file1.base_hash, file2.base_hash);
    assert_ne!(file1, file2);
}

#[test]
fn test_file_node_hash_uniqueness() {
    let file1 = FileNode::new("test.txt".into(), b"content 1");
    let file2 = FileNode::new("test.txt".into(), b"content 2");

    assert_ne!(file1.base_hash, file2.base_hash);
}

#[test]
fn test_file_node_empty_content() {
    let content = b"";
    let file = FileNode::new("empty.txt".into(), content);

    assert_ne!(file.base_hash, [0u8; 32]);
}

#[test]
fn test_file_node_large_content() {
    let content: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
    let file = FileNode::new("large.bin".into(), &content);

    assert_ne!(file.base_hash, [0u8; 32]);
}

#[test]
fn test_file_node_equality() {
    let content = b"test content";
    let file1 = FileNode::new("test.txt".into(), content);
    let file2 = FileNode::new("test.txt".into(), content);

    assert_eq!(file1, file2);
}

#[test]
fn test_file_node_path_str() {
    let file = FileNode::new("src/main.rs".into(), b"content");
    assert_eq!(file.path_str(), "src/main.rs");
}

#[test]
fn test_file_node_serialization() {
    let file = FileNode::new("test.txt".into(), b"content");

    let json = serde_json::to_string(&file).unwrap();
    let file2: FileNode = serde_json::from_str(&json).unwrap();

    assert_eq!(file, file2);
}

#[test]
fn test_file_node_hash() {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let file = FileNode::new("test.txt".into(), b"content");

    let mut hasher = DefaultHasher::new();
    file.hash(&mut hasher);
    let hash1 = hasher.finish();

    hasher = DefaultHasher::new();
    file.hash(&mut hasher);
    let hash2 = hasher.finish();

    assert_eq!(hash1, hash2);
}
