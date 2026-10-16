//! Directory/file tree helpers for diff viewers and resource browsers.

/// One node in a file tree.
#[derive(Debug, Clone)]
pub struct FileTreeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub expanded: bool,
    pub children: Vec<Self>,
}

impl FileTreeNode {
    pub fn new(name: impl Into<String>, path: impl Into<String>, is_dir: bool) -> Self {
        Self {
            name: name.into(),
            path: path.into(),
            is_dir,
            expanded: is_dir,
            children: Vec::new(),
        }
    }

    pub fn dir(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self::new(name, path, true)
    }

    pub fn file(name: impl Into<String>, path: impl Into<String>) -> Self {
        Self::new(name, path, false)
    }

    pub fn add_child(&mut self, child: Self) {
        self.children.push(child);
    }

    /// Flat, indented rendering of the visible (expanded) tree.
    pub fn flatten(&self) -> Vec<FileTreeRow> {
        let mut out = Vec::new();
        self.flatten_into(&mut out, 0);
        out
    }

    fn flatten_into(&self, out: &mut Vec<FileTreeRow>, depth: usize) {
        out.push(FileTreeRow {
            name: self.name.clone(),
            path: self.path.clone(),
            is_dir: self.is_dir,
            depth,
            expanded: self.expanded,
        });
        if self.expanded {
            for child in &self.children {
                child.flatten_into(out, depth + 1);
            }
        }
    }
}

/// A flattened, display-ready row of a file tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileTreeRow {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

impl FileTreeRow {
    pub fn indent(&self) -> String {
        "  ".repeat(self.depth)
    }

    pub fn label(&self) -> String {
        let marker = if self.is_dir {
            if self.expanded {
                "v"
            } else {
                ">"
            }
        } else {
            " "
        };
        format!("{}{} {}", self.indent(), marker, self.name)
    }
}

/// Build a tree from (path, is_dir) pairs.
pub fn build_tree(entries: &[(String, bool)]) -> Vec<FileTreeNode> {
    let mut roots: Vec<FileTreeNode> = Vec::new();
    for (path, is_dir) in entries {
        insert_path(&mut roots, path, *is_dir);
    }
    roots
}

fn insert_path(roots: &mut Vec<FileTreeNode>, path: &str, is_dir: bool) {
    let parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    if parts.is_empty() {
        return;
    }
    let mut current = roots;
    let mut accumulated = String::new();
    for (index, part) in parts.iter().enumerate() {
        if !accumulated.is_empty() {
            accumulated.push('/');
        }
        accumulated.push_str(part);
        let last = index == parts.len() - 1;
        let node_is_dir = if last { is_dir } else { true };
        let node_path = accumulated.clone();
        let pos = current.iter().position(|n| n.name == *part);
        if let Some(pos) = pos {
            if last {
                current[pos].is_dir = node_is_dir;
            }
            // obtain mutable child reference by splitting borrow via index
            // (avoid simultaneous borrows)
            let children = &mut current[pos].children;
            current = children;
        } else {
            current.push(FileTreeNode::new(*part, node_path, node_is_dir));
            let idx = current.len() - 1;
            if last {
                current[idx].is_dir = node_is_dir;
            }
            // re-borrow after push
            // can't mutate current len and then borrow in same expression; do in two steps
            // placeholder handled below
            let children = &mut current[idx].children;
            current = children;
        }
    }
}

/// Collect a flat list of changed file paths for a git-style status list.
pub fn changed_files(rows: &[FileTreeRow]) -> Vec<String> {
    rows.iter().map(|row| row.path.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flatten_respects_expansion() {
        let mut root = FileTreeNode::dir("src", "src");
        root.add_child(FileTreeNode::file("main.rs", "src/main.rs"));
        let rows = root.flatten();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].depth, 1);
    }

    #[test]
    fn collapsed_dir_hides_children() {
        let mut root = FileTreeNode::dir("src", "src");
        root.expanded = false;
        root.add_child(FileTreeNode::file("main.rs", "src/main.rs"));
        assert_eq!(root.flatten().len(), 1);
    }

    #[test]
    fn build_tree_nests_paths() {
        let entries = vec![("src/main.rs".into(), false), ("Cargo.toml".into(), false)];
        let roots = build_tree(&entries);
        assert_eq!(roots.len(), 2);
        let src = roots.iter().find(|n| n.name == "src").unwrap();
        assert_eq!(src.children.len(), 1);
    }

    #[test]
    fn labels_indent_nested_rows() {
        let row = FileTreeRow {
            name: "main.rs".into(),
            path: "src/main.rs".into(),
            is_dir: false,
            depth: 1,
            expanded: false,
        };
        assert_eq!(row.label(), "    main.rs");
    }
}
