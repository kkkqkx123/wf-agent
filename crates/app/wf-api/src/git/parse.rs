use super::types::{
    BranchEntry, CommitEntry, CommitFileEntry, DiffFileView, DiffHunk, FileEntry, FileKind,
    StashEntry, TagEntry,
};

const LOG_SEP: &str = "\u{1f}";
const LOG_END: &str = "\u{1e}";

pub fn short_commit(id: &str) -> String {
    id.chars().take(7).collect()
}

pub fn parse_branch_header(line: &str) -> (String, Option<String>, u32, u32) {
    let mut branch = String::new();
    let mut upstream: Option<String> = None;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    let body = line.strip_prefix("# branch.head ").unwrap_or("").trim();
    if !body.is_empty() && body != "(unknown)" && body != "(no branch)" {
        branch = body.to_string();
    }
    branch = branch.trim().to_string();
    let _ = (&mut upstream, &mut ahead, &mut behind);
    (branch, upstream, ahead, behind)
}

pub fn parse_porcelain_v2_branch_info(output: &str) -> (String, Option<String>, u32, u32) {
    let mut branch = String::new();
    let mut upstream: Option<String> = None;
    let mut ahead = 0u32;
    let mut behind = 0u32;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            let name = rest.trim();
            if !name.is_empty() && name != "(unknown)" && name != "(no branch)" {
                branch = name.to_string();
            }
        } else if let Some(rest) = line.strip_prefix("# branch.upstream ") {
            let name = rest.trim();
            if !name.is_empty() {
                upstream = Some(name.to_string());
            }
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for part in rest.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    behind = n.parse().unwrap_or(0);
                }
            }
        }
    }
    (branch, upstream, ahead, behind)
}

pub fn parse_porcelain_v2_files(output: &str) -> Vec<FileEntry> {
    let mut out = Vec::new();
    for line in output.lines() {
        if line.starts_with("1 ") || line.starts_with("2 ") {
            let parts: Vec<&str> = line.splitn(9, ' ').collect();
            if parts.len() < 9 {
                continue;
            }
            let xy = parts[1];
            let mut xy_chars = xy.chars();
            let staged = xy_chars.next().unwrap_or('.').to_string();
            let unstaged = xy_chars.next().unwrap_or('.').to_string();
            let path_field = parts[8];
            let (path, renamed_from) = match path_field.split_once('\t') {
                Some(_) => {
                    let mut it = path_field.split('\t');
                    let _score = it.next().unwrap_or("");
                    let new_path = it.next().unwrap_or("").to_string();
                    let old_path = it.next().unwrap_or("").to_string();
                    if old_path.is_empty() {
                        (new_path, None)
                    } else {
                        (new_path.clone(), Some(old_path))
                    }
                }
                None => {
                    let mut it = path_field.split('\t');
                    let p = it.next().unwrap_or("").to_string();
                    (p, None)
                }
            };
            let kind = if staged == "U" || unstaged == "U" || (staged == "A" && unstaged == "A") {
                FileKind::Conflicted
            } else if renamed_from.is_some() {
                FileKind::Renamed
            } else if staged != "." && staged != "?" {
                FileKind::Staged
            } else {
                FileKind::Unstaged
            };
            out.push(FileEntry {
                path,
                staged,
                unstaged,
                kind,
                renamed_from,
            });
        } else if line.starts_with("? ") {
            let path = line[2..].to_string();
            out.push(FileEntry {
                path,
                staged: "?".to_string(),
                unstaged: "?".to_string(),
                kind: FileKind::Untracked,
                renamed_from: None,
            });
        } else if line.starts_with("u ") {
            let rest = line[2..].trim().to_string();
            let path = rest.split_whitespace().last().unwrap_or("").to_string();
            if !path.is_empty() {
                out.push(FileEntry {
                    path,
                    staged: "U".to_string(),
                    unstaged: "U".to_string(),
                    kind: FileKind::Conflicted,
                    renamed_from: None,
                });
            }
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

pub fn parse_diff_unified(output: &str) -> Vec<DiffFileView> {
    use super::types::MAX_DIFF_LINES;
    struct Accum {
        file: DiffFileView,
        hunk: Option<DiffHunk>,
        lines: usize,
        truncated: bool,
        binary: bool,
        old_null: bool,
        new_null: bool,
    }
    impl Accum {
        fn finish(mut self, additions: usize, deletions: usize) -> DiffFileView {
            if let Some(hunk) = self.hunk.take() {
                self.file.hunks.push(hunk);
            }
            self.file.additions = additions;
            self.file.deletions = deletions;
            self.file.truncated = self.truncated;
            self.file.binary = self.binary;
            self.file.kind = if self.old_null && !self.new_null {
                "added".to_string()
            } else if self.new_null && !self.old_null {
                "deleted".to_string()
            } else {
                "modified".to_string()
            };
            self.file
        }
        fn push_line(&mut self, line: &str) {
            if self.lines >= MAX_DIFF_LINES {
                self.truncated = true;
                return;
            }
            if let Some(hunk) = self.hunk.as_mut() {
                hunk.lines.push(line.to_string());
                self.lines += 1;
            }
        }
    }
    let mut files: Vec<DiffFileView> = Vec::new();
    let mut current: Option<Accum> = None;
    let mut additions = 0usize;
    let mut deletions = 0usize;
    for line in output.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some(accum) = current.take() {
                files.push(accum.finish(additions, deletions));
            }
            additions = 0;
            deletions = 0;
            let path = rest
                .rsplit(" b/")
                .next()
                .unwrap_or(rest)
                .trim_start_matches("a/")
                .to_string();
            current = Some(Accum {
                file: DiffFileView {
                    path,
                    kind: "modified".to_string(),
                    hunks: Vec::new(),
                    additions: 0,
                    deletions: 0,
                    binary: false,
                    truncated: false,
                },
                hunk: None,
                lines: 0,
                truncated: false,
                binary: false,
                old_null: false,
                new_null: false,
            });
        } else if line.starts_with("@@ ") {
            if let Some(accum) = current.as_mut() {
                if let Some(hunk) = accum.hunk.take() {
                    accum.file.hunks.push(hunk);
                }
                accum.hunk = Some(DiffHunk {
                    header: line.to_string(),
                    lines: Vec::new(),
                });
            }
        } else if line.starts_with("Binary files ") {
            // Binary content carries no hunks; the flag survives even
            // when no text lines were collected.
            if let Some(accum) = current.as_mut() {
                if let Some(hunk) = accum.hunk.take() {
                    accum.file.hunks.push(hunk);
                }
                accum.binary = true;
            }
        } else if line.starts_with("--- ") {
            if let Some(accum) = current.as_mut() {
                accum.old_null = line.trim_end() == "--- /dev/null";
            }
        } else if line.starts_with("+++ ") {
            if let Some(accum) = current.as_mut() {
                accum.new_null = line.trim_end() == "+++ /dev/null";
            }
        } else if line.starts_with('+') {
            additions += 1;
            if let Some(accum) = current.as_mut() {
                accum.push_line(line);
            }
        } else if line.starts_with('-') {
            deletions += 1;
            if let Some(accum) = current.as_mut() {
                accum.push_line(line);
            }
        } else if let Some(accum) = current.as_mut() {
            // Context and metadata lines belong to the open hunk; header
            // lines outside any hunk are dropped.
            if accum.hunk.is_some() {
                accum.push_line(line);
            }
        }
    }
    if let Some(accum) = current.take() {
        files.push(accum.finish(additions, deletions));
    }
    files
}

pub fn parse_log_entries(output: &str) -> Vec<CommitEntry> {
    let mut out = Vec::new();
    for record in output.split(LOG_END) {
        let record = record.trim_matches(|c| c == '\n' || c == '\r');
        if record.trim().is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split(LOG_SEP).collect();
        if fields.len() < 7 {
            continue;
        }
        let id = fields[0].trim().to_string();
        if id.is_empty() {
            continue;
        }
        let parents = if fields[1].trim().is_empty() {
            Vec::new()
        } else {
            fields[1].split_whitespace().map(str::to_string).collect()
        };
        let author = fields[2].to_string();
        let email = fields[3].to_string();
        let timestamp = fields[4].trim().parse().unwrap_or(0);
        let subject = fields[5].to_string();
        let body = fields.get(6).cloned().unwrap_or("").to_string();
        let refs = fields
            .get(7)
            .map(|s| {
                s.split(", ")
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        out.push(CommitEntry {
            short_id: short_commit(&id),
            id,
            author,
            email,
            timestamp,
            subject,
            body,
            parents,
            refs,
        });
    }
    out
}

pub fn log_format_args() -> Vec<String> {
    vec![
        format!("--format=%H{LOG_SEP}%P{LOG_SEP}%an{LOG_SEP}%ae{LOG_SEP}%ct{LOG_SEP}%s{LOG_SEP}%b{LOG_SEP}%D{LOG_END}"),
    ]
}

pub fn parse_branches(output: &str) -> Vec<BranchEntry> {
    let mut out = Vec::new();
    for record in output.split(LOG_END) {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split(LOG_SEP).collect();
        if fields.len() < 5 {
            continue;
        }
        let (ahead, behind, commit_idx, subject_idx) = if fields.len() >= 7 {
            (
                fields[3].trim().parse().unwrap_or(0),
                fields[4].trim().parse().unwrap_or(0),
                5,
                6,
            )
        } else {
            (0, 0, 3, 4)
        };
        out.push(BranchEntry {
            name: fields[0].trim().to_string(),
            current: fields[1].trim() == "*",
            upstream: {
                let u = fields[2].trim();
                if u.is_empty() {
                    None
                } else {
                    Some(u.to_string())
                }
            },
            ahead,
            behind,
            commit: fields[commit_idx].trim().to_string(),
            subject: fields
                .get(subject_idx)
                .cloned()
                .unwrap_or("")
                .trim()
                .to_string(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn parse_tags(output: &str) -> Vec<TagEntry> {
    let mut out = Vec::new();
    for record in output.split(LOG_END) {
        let record = record.trim();
        if record.is_empty() {
            continue;
        }
        let fields: Vec<&str> = record.split(LOG_SEP).collect();
        if fields.len() < 3 {
            continue;
        }
        let peeled = fields.get(2).map(|s| s.trim()).unwrap_or("");
        let commit = if peeled.is_empty() {
            fields[1].trim().to_string()
        } else {
            peeled.to_string()
        };
        out.push(TagEntry {
            name: fields[0].trim().to_string(),
            commit,
            message: fields
                .get(3)
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

pub fn parse_stash_list(output: &str) -> Vec<StashEntry> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("stash@{") {
            if let Some(end) = rest.find('}') {
                let index: usize = rest[..end].parse().unwrap_or(out.len());
                let message = rest[end + 1..].trim_start_matches(':').trim().to_string();
                out.push(StashEntry {
                    index,
                    name: format!("stash@{{{index}}}"),
                    message,
                });
            }
        }
    }
    out
}

pub fn parse_name_status(output: &str) -> Vec<CommitFileEntry> {
    let mut out = Vec::new();
    for line in output.lines() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.split('\t');
        let status = parts.next().unwrap_or("").trim().to_string();
        if status.is_empty() {
            continue;
        }
        let first = parts.next().unwrap_or("").to_string();
        let second = parts.next();
        if status.starts_with('R') || status.starts_with('C') {
            out.push(CommitFileEntry {
                path: second.unwrap_or("").to_string(),
                status,
                renamed_from: Some(first),
            });
        } else {
            out.push(CommitFileEntry {
                path: first,
                status,
                renamed_from: None,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_v2_parses_branch_and_files() {
        let output = "# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n1 .M N... 100644 100644 100644 abc def src/a.rs\n? untracked.txt\n";
        let (branch, upstream, ahead, behind) = parse_porcelain_v2_branch_info(output);
        assert_eq!(branch, "main");
        assert_eq!(upstream.as_deref(), Some("origin/main"));
        assert_eq!((ahead, behind), (2, 1));
        let files = parse_porcelain_v2_files(output);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[1].kind, FileKind::Untracked);
    }

    #[test]
    fn porcelain_v2_covers_empty_detached_and_conflict() {
        let (branch, upstream, ahead, behind) =
            parse_porcelain_v2_branch_info("# branch.head (unknown)\n");
        assert!(branch.is_empty());
        assert!(upstream.is_none());
        assert_eq!((ahead, behind), (0, 0));
        let files =
            parse_porcelain_v2_files("u UU N... 100644 100644 100644 100644 a b src/x.rs\n");
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, FileKind::Conflicted);
    }

    #[test]
    fn diff_parses_hunks_with_counts() {
        let output =
            "diff --git a/src/a.rs b/src/a.rs\n@@ -1,2 +1,3 @@\n-old\n+new\n+extra\n context\n";
        let files = parse_diff_unified(output);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/a.rs");
        assert_eq!(files[0].hunks.len(), 1);
        assert_eq!(files[0].additions, 2);
        assert_eq!(files[0].deletions, 1);
        assert_eq!(files[0].kind, "modified");
        assert!(!files[0].binary);
        assert!(!files[0].truncated);
    }

    #[test]
    fn diff_derives_kind_from_null_headers() {
        let added = "diff --git a/new.rs b/new.rs\nnew file mode 100644\n--- /dev/null\n+++ b/new.rs\n@@ -0,0 +1 @@\n+hi\n";
        let files = parse_diff_unified(added);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, "added");
        assert!(!files[0].binary);
        let deleted = "diff --git a/old.rs b/old.rs\ndeleted file mode 100644\n--- a/old.rs\n+++ /dev/null\n@@ -1 +0,0 @@\n-bye\n";
        let files = parse_diff_unified(deleted);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].kind, "deleted");
    }

    #[test]
    fn diff_marks_binary_and_truncates_long_hunks() {
        let binary =
            "diff --git a/img.png b/img.png\nBinary files a/img.png and b/img.png differ\n";
        let files = parse_diff_unified(binary);
        assert_eq!(files.len(), 1);
        assert!(files[0].binary);
        assert!(files[0].hunks.is_empty());
        let mut big = "diff --git a/big.rs b/big.rs\n@@ -1 +1 @@\n".to_string();
        for i in 0..(super::super::types::MAX_DIFF_LINES + 10) {
            big.push_str(&format!("+line{i}\n"));
        }
        let files = parse_diff_unified(&big);
        assert_eq!(files.len(), 1);
        assert!(files[0].truncated);
        assert_eq!(
            files[0].hunks.iter().map(|h| h.lines.len()).sum::<usize>(),
            super::super::types::MAX_DIFF_LINES
        );
    }

    #[test]
    fn log_parses_custom_separator() {
        let output = format!(
            "abc123{LOG_SEP}def456{LOG_SEP}Alice{LOG_SEP}a@x{LOG_SEP}1700000000{LOG_SEP}subj{LOG_SEP}body{LOG_SEP}HEAD -> main{LOG_END}"
        );
        let entries = parse_log_entries(&output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "abc123");
        assert_eq!(entries[0].refs, vec!["HEAD -> main".to_string()]);
    }

    #[test]
    fn branches_tags_stash_parse() {
        let branches = parse_branches(&format!(
            "main{LOG_SEP}*{LOG_SEP}origin/main{LOG_SEP}2{LOG_SEP}0{LOG_SEP}abc{LOG_SEP}subj{LOG_END}"
        ));
        assert_eq!(branches.len(), 1);
        assert!(branches[0].current);
        assert_eq!((branches[0].ahead, branches[0].behind), (2, 0));
        let short = parse_branches(&format!(
            "main{LOG_SEP}*{LOG_SEP}origin/main{LOG_SEP}abc{LOG_SEP}subj{LOG_END}"
        ));
        assert_eq!(short.len(), 1);
        assert_eq!((short[0].ahead, short[0].behind), (0, 0));
        let tags = parse_tags(&format!("v1{LOG_SEP}abc{LOG_SEP}{LOG_SEP}msg{LOG_END}"));
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].commit, "abc");
        let peeled = parse_tags(&format!(
            "v2{LOG_SEP}tagobj{LOG_SEP}commit123{LOG_SEP}msg{LOG_END}"
        ));
        assert_eq!(peeled[0].commit, "commit123");
        let stash = parse_stash_list("stash@{0}: WIP on main: abc first\n");
        assert_eq!(stash.len(), 1);
        assert_eq!(stash[0].index, 0);
    }

    #[test]
    fn name_status_handles_renames() {
        let files = parse_name_status("R100\tsrc/a.rs\tsrc/b.rs\nM\tsrc/c.rs\n");
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].renamed_from.as_deref(), Some("src/a.rs"));
        assert_eq!(files[0].path, "src/b.rs");
    }
}
