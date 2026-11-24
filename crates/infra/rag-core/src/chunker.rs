//! Recursive chunking with optional hierarchical parents.
//!
//! The default splitter walks the separator list from coarse to fine and
//! keeps pieces within the target length, mirroring the Haystack recursive
//! splitter parameter shape. Hierarchical mode additionally stores one
//! parent chunk per top-level section so auto-merge promotion has content
//! to promote to.

use std::collections::HashMap;

use crate::document::{chunk_id_for, parent_chunk_id_for, RagChunk, RagDocument};
use crate::error::{RagError, Result};

/// Chunking parameters.
#[derive(Debug, Clone)]
pub struct ChunkerConfig {
    /// Soft target length of a leaf chunk in characters.
    pub target_chars: usize,
    /// Overlap between consecutive leaf chunks in characters.
    pub overlap_chars: usize,
    /// Optional token budget per leaf chunk counted on whitespace tokens.
    /// Unset keeps character sizing only.
    pub token_budget: Option<usize>,
    /// Separators tried from coarse to fine.
    pub separators: Vec<String>,
    /// Separators defining top-level sections for hierarchical parents.
    pub parent_separators: Vec<String>,
    /// Whether document metadata is copied onto every chunk.
    pub keep_metadata: bool,
    /// Whether prev/next links are recorded between leaf chunks.
    pub keep_relations: bool,
    /// Adds `heading` and `heading_path` metadata from Markdown headings.
    pub heading_paths: bool,
    /// Keeps Markdown table rows and fenced blocks in one piece.
    pub table_blocks: bool,
}

impl Default for ChunkerConfig {
    fn default() -> Self {
        Self {
            target_chars: 800,
            overlap_chars: 100,
            separators: vec![
                "\n\n".to_string(),
                "\n".to_string(),
                ". ".to_string(),
                " ".to_string(),
            ],
            parent_separators: vec!["\n\n".to_string(), "\n".to_string()],
            keep_metadata: true,
            keep_relations: true,
            token_budget: None,
            heading_paths: false,
            table_blocks: false,
        }
    }
}

impl ChunkerConfig {
    /// Named preset splitting on Markdown headings first, then paragraphs
    /// and lines. Shares the recursive pipeline; only the separator order
    /// and section boundaries differ.
    pub fn markdown() -> Self {
        Self {
            separators: vec![
                "\n## ".to_string(),
                "\n# ".to_string(),
                "\n\n".to_string(),
                "\n".to_string(),
                ". ".to_string(),
                " ".to_string(),
            ],
            parent_separators: vec!["\n## ".to_string(), "\n# ".to_string(), "\n\n".to_string()],
            heading_paths: true,
            ..Self::default()
        }
    }

    /// Named preset splitting on sentence boundaries first, then on
    /// whitespace. Shares the recursive pipeline; only the separator
    /// order differs.
    pub fn sentence() -> Self {
        Self {
            separators: vec![
                ". ".to_string(),
                "! ".to_string(),
                "? ".to_string(),
                "\n".to_string(),
                " ".to_string(),
            ],
            ..Self::default()
        }
    }

    /// Named preset for source code: blank lines and block or statement
    /// boundaries split before generic punctuation. Approximation only;
    /// syntax-aware splitting stays with external parsers.
    pub fn code() -> Self {
        Self {
            separators: vec![
                "\n\n".to_string(),
                "}\n".to_string(),
                "{\n".to_string(),
                ";\n".to_string(),
                "\n".to_string(),
                ". ".to_string(),
                " ".to_string(),
            ],
            parent_separators: vec!["\n\n".to_string()],
            ..Self::default()
        }
    }

    /// Named preset that keeps Markdown tables and fenced code blocks
    /// intact while still splitting prose on paragraphs and sentences.
    pub fn table() -> Self {
        Self {
            separators: vec!["\n\n".to_string(), ". ".to_string(), " ".to_string()],
            parent_separators: vec!["\n\n".to_string()],
            table_blocks: true,
            ..Self::default()
        }
    }
}

impl ChunkerConfig {
    /// Validates the numeric parameters.
    pub fn validate(&self) -> Result<()> {
        if self.target_chars == 0 {
            return Err(RagError::InvalidInput(
                "target_chars must be positive".to_string(),
            ));
        }
        if self.overlap_chars >= self.target_chars {
            return Err(RagError::InvalidInput(
                "overlap_chars must be smaller than target_chars".to_string(),
            ));
        }
        if self.separators.is_empty() {
            return Err(RagError::InvalidInput(
                "separators must not be empty".to_string(),
            ));
        }
        if self.token_budget.is_some_and(|budget| budget == 0) {
            return Err(RagError::InvalidInput(
                "token_budget must be positive".to_string(),
            ));
        }
        Ok(())
    }
}

/// Recursive splitter producing linked leaf chunks.
#[derive(Debug, Clone)]
pub struct RecursiveChunker {
    config: ChunkerConfig,
}

impl RecursiveChunker {
    /// Builds a splitter with the given config.
    pub fn new(config: ChunkerConfig) -> Result<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    /// Splits documents into linked leaf chunks.
    pub fn split_documents(&self, documents: &[RagDocument]) -> Result<Vec<RagChunk>> {
        let mut chunks = Vec::new();
        for document in documents {
            document.validate()?;
            chunks.extend(self.split_one(document));
        }
        Ok(chunks)
    }

    /// Splits documents into leaf chunks plus one stored parent chunk per
    /// top-level section. Leaf chunks carry the parent id for promotion.
    pub fn split_hierarchical(&self, documents: &[RagDocument]) -> Result<Vec<RagChunk>> {
        let mut chunks = Vec::new();
        for document in documents {
            document.validate()?;
            let sections = split_once(&document.content, &self.config.parent_separators);
            for (section_ordinal, section) in sections.iter().enumerate() {
                let text = section.trim();
                if text.is_empty() {
                    continue;
                }
                let parent_id = parent_chunk_id_for(&document.source, section_ordinal);
                let leaves = self.split_text(text);
                let base = chunks.len();
                for (index, content) in leaves.iter().enumerate() {
                    chunks.push(RagChunk {
                        id: chunk_id_for(&document.source, base + index),
                        document_id: document.id.clone(),
                        ordinal: base + index,
                        content: content.clone(),
                        metadata: self.metadata_for(document, text),
                        prev_id: None,
                        next_id: None,
                        parent_id: Some(parent_id.clone()),
                        is_parent: false,
                    });
                }
                self.link_chunks(&mut chunks, base);
                chunks.push(RagChunk {
                    id: parent_id,
                    document_id: document.id.clone(),
                    ordinal: base + leaves.len(),
                    content: text.to_string(),
                    metadata: self.metadata_for(document, text),
                    prev_id: None,
                    next_id: None,
                    parent_id: None,
                    is_parent: true,
                });
            }
        }
        Ok(chunks)
    }

    fn split_one(&self, document: &RagDocument) -> Vec<RagChunk> {
        let sections = if self.config.heading_paths {
            markdown_sections(&document.content)
        } else {
            vec![(document.content.clone(), Vec::new())]
        };
        let mut chunks = Vec::new();
        for (section, heading_path) in sections {
            let pieces = self.split_text(&section);
            let base = chunks.len();
            chunks.extend(pieces.into_iter().enumerate().map(|(index, content)| {
                let ordinal = base + index;
                RagChunk {
                    id: chunk_id_for(&document.source, ordinal),
                    document_id: document.id.clone(),
                    ordinal,
                    content,
                    metadata: self.metadata_for_section(document, &section, &heading_path),
                    prev_id: None,
                    next_id: None,
                    parent_id: None,
                    is_parent: false,
                }
            }));
        }
        self.link_all(&mut chunks);
        chunks
    }

    fn metadata_for(&self, document: &RagDocument, context: &str) -> HashMap<String, String> {
        let (path, _) = heading_context(context);
        self.metadata_for_headings(document, &path)
    }

    fn metadata_for_section(
        &self,
        document: &RagDocument,
        context: &str,
        inherited_path: &[String],
    ) -> HashMap<String, String> {
        let path = if inherited_path.is_empty() {
            heading_context(context).0
        } else {
            inherited_path.join(" / ")
        };
        self.metadata_for_headings(document, &path)
    }

    fn metadata_for_headings(&self, document: &RagDocument, path: &str) -> HashMap<String, String> {
        let mut metadata = if self.config.keep_metadata {
            document.metadata.clone()
        } else {
            HashMap::new()
        };
        if self.config.heading_paths {
            if let Some(heading) = path.rsplit(" / ").next() {
                if !heading.is_empty() {
                    metadata.insert("heading".to_string(), heading.to_string());
                    metadata.insert("heading_path".to_string(), path.to_string());
                }
            }
        }
        metadata
    }

    fn link_chunks(&self, chunks: &mut [RagChunk], base: usize) {
        if !self.config.keep_relations {
            return;
        }
        let len = chunks.len() - base;
        for offset in 0..len {
            let index = base + offset;
            if offset > 0 {
                chunks[index].prev_id = Some(chunks[index - 1].id.clone());
            }
            if offset + 1 < len && !chunks[index].is_parent && !chunks[index + 1].is_parent {
                chunks[index].next_id = Some(chunks[index + 1].id.clone());
            }
        }
    }

    fn link_all(&self, chunks: &mut [RagChunk]) {
        if !self.config.keep_relations {
            return;
        }
        for index in 0..chunks.len() {
            chunks[index].prev_id = index.checked_sub(1).map(|prev| chunks[prev].id.clone());
            chunks[index].next_id = chunks.get(index + 1).map(|next| next.id.clone());
        }
    }

    fn split_text(&self, text: &str) -> Vec<String> {
        if !self.config.table_blocks {
            return self.split_sized(text);
        }
        protected_segments(text)
            .into_iter()
            .flat_map(|(segment, atomic)| {
                if atomic {
                    vec![segment]
                } else {
                    self.split_sized(&segment)
                }
            })
            .collect()
    }

    fn split_sized(&self, text: &str) -> Vec<String> {
        let pieces = self.split_plain(text);
        let overlapped = apply_overlap(pieces, self.config.target_chars, self.config.overlap_chars);
        match self.config.token_budget {
            Some(budget) => apply_token_budget(overlapped, budget),
            None => overlapped,
        }
    }

    fn split_plain(&self, text: &str) -> Vec<String> {
        let mut pieces = vec![text.to_string()];
        for separator in &self.config.separators {
            let mut refined = Vec::new();
            for piece in &pieces {
                if piece.chars().count() <= self.config.target_chars {
                    refined.push(piece.clone());
                    continue;
                }
                let splits: Vec<String> = if separator.is_empty() {
                    vec![piece.clone()]
                } else {
                    piece
                        .split(separator)
                        .map(|part| part.trim().to_string())
                        .filter(|part| !part.is_empty())
                        .collect()
                };
                if splits.len() <= 1 {
                    refined.push(piece.clone());
                } else {
                    refined.extend(splits);
                }
            }
            pieces = refined;
        }
        pieces
    }
}

/// Splits Markdown into sections rooted at each ATX heading, keeping the
/// heading line so both chunk content and metadata retain its context.
fn markdown_sections(text: &str) -> Vec<(String, Vec<String>)> {
    let mut sections = Vec::new();
    let mut current = String::new();
    let mut path: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        if let Some(level) = markdown_heading_level(line) {
            if !current.trim().is_empty() {
                sections.push((
                    std::mem::take(&mut current),
                    path.iter().map(|(_, title)| title.clone()).collect(),
                ));
            }
            let title = line.trim_start().trim_start_matches('#').trim();
            path.retain(|(existing, _)| *existing < level);
            path.push((level, title.to_string()));
        }
        current.push_str(line);
        current.push('\n');
    }
    if !current.trim().is_empty() {
        sections.push((
            current,
            path.iter().map(|(_, title)| title.clone()).collect(),
        ));
    }
    sections
}

fn markdown_heading_level(line: &str) -> Option<usize> {
    let rest = line.trim_start().strip_prefix('#')?;
    let level = 1 + rest.chars().take_while(|c| *c == '#').count();
    (level <= 6 && rest[level - 1..].starts_with(' ')).then_some(level)
}

/// Derives the current Markdown heading and its full path from a section.
fn heading_context(text: &str) -> (String, Option<String>) {
    let mut path: Vec<(usize, String)> = Vec::new();
    for line in text.lines() {
        let trimmed = line.trim();
        let Some(rest) = trimmed.strip_prefix('#') else {
            continue;
        };
        let level = 1 + rest.chars().take_while(|c| *c == '#').count();
        if level > 6 {
            continue;
        }
        let title = rest[level - 1..].trim().trim_end_matches('#').trim();
        if title.is_empty() {
            continue;
        }
        path.retain(|(existing, _)| *existing < level);
        path.push((level, title.to_string()));
    }
    let joined = path
        .iter()
        .map(|(_, title)| title.as_str())
        .collect::<Vec<_>>()
        .join(" / ");
    let heading = path.last().map(|(_, title)| title.clone());
    (joined, heading)
}

/// Splits text into prose and atomic table/fence segments so table rows
/// stay together and bypass the recursive separator pass.
fn protected_segments(text: &str) -> Vec<(String, bool)> {
    let mut segments = Vec::new();
    let mut prose = String::new();
    let mut block = String::new();
    let mut in_fence = false;
    let mut in_table = false;

    let flush_prose = |prose: &mut String, segments: &mut Vec<(String, bool)>| {
        if !prose.trim().is_empty() {
            segments.push((std::mem::take(prose), false));
        }
    };
    let flush_block = |block: &mut String, segments: &mut Vec<(String, bool)>| {
        if !block.trim().is_empty() {
            segments.push((std::mem::take(block), true));
        }
    };

    for line in text.lines() {
        let trimmed = line.trim_start();
        let is_fence = trimmed.starts_with("```");
        let is_table = trimmed.starts_with('|');
        if in_fence {
            block.push_str(line);
            block.push('\n');
            if is_fence {
                in_fence = false;
            }
            continue;
        }
        if is_fence {
            flush_prose(&mut prose, &mut segments);
            block.push_str(line);
            block.push('\n');
            in_fence = true;
            continue;
        }
        if is_table {
            flush_prose(&mut prose, &mut segments);
            block.push_str(line);
            block.push('\n');
            in_table = true;
            continue;
        }
        if in_table {
            flush_block(&mut block, &mut segments);
            in_table = false;
        }
        prose.push_str(line);
        prose.push('\n');
    }
    flush_block(&mut block, &mut segments);
    flush_prose(&mut prose, &mut segments);
    segments
}

/// Splits once on the first separator producing more than one piece.
fn split_once(text: &str, separators: &[String]) -> Vec<String> {
    for separator in separators {
        if separator.is_empty() {
            continue;
        }
        let parts: Vec<String> = text
            .split(separator)
            .map(|part| part.to_string())
            .filter(|part| !part.trim().is_empty())
            .collect();
        if parts.len() > 1 {
            return parts;
        }
    }
    vec![text.to_string()]
}

/// Merges tiny pieces and applies character overlap between neighbors.
fn apply_overlap(pieces: Vec<String>, target: usize, overlap: usize) -> Vec<String> {
    let mut merged: Vec<String> = Vec::new();
    for piece in pieces {
        let fits = merged
            .last()
            .is_some_and(|last| last.chars().count() + 1 + piece.chars().count() <= target);
        if fits {
            if let Some(last) = merged.last_mut() {
                last.push(' ');
                last.push_str(&piece);
            }
        } else {
            merged.push(piece);
        }
    }
    if overlap == 0 || merged.len() < 2 {
        return merged;
    }
    let mut overlapped = Vec::with_capacity(merged.len());
    for (index, piece) in merged.iter().enumerate() {
        if index == 0 {
            overlapped.push(piece.clone());
            continue;
        }
        let prev_chars: Vec<char> = overlapped[index - 1].chars().collect();
        let take = overlap.min(prev_chars.len());
        let prefix: String = prev_chars[prev_chars.len() - take..].iter().collect();
        overlapped.push(format!("{prefix}{piece}"));
    }
    overlapped
}

/// Counts whitespace-separated tokens in a text.
pub fn count_tokens(text: &str) -> usize {
    text.split_whitespace()
        .filter(|token| !token.is_empty())
        .count()
}

/// Re-splits pieces exceeding the token budget on whitespace boundaries.
/// Budget-sized token runs join with single spaces; smaller pieces pass
/// through unchanged.
fn apply_token_budget(pieces: Vec<String>, budget: usize) -> Vec<String> {
    let mut out = Vec::with_capacity(pieces.len());
    for piece in pieces {
        let tokens: Vec<&str> = piece.split_whitespace().collect();
        if tokens.len() <= budget {
            out.push(piece);
            continue;
        }
        for run in tokens.chunks(budget) {
            out.push(run.join(" "));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(content: &str) -> RagDocument {
        RagDocument::new("d1", content, "doc.md")
    }

    #[test]
    fn short_text_stays_one_chunk() {
        let chunker = RecursiveChunker::new(ChunkerConfig::default()).expect("config");
        let chunks = chunker
            .split_documents(&[doc("hello world")])
            .expect("split");
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].id, "doc.md#0");
        assert!(chunks[0].prev_id.is_none() && chunks[0].next_id.is_none());
    }

    #[test]
    fn long_text_splits_and_links_neighbors() {
        let text = (0..20)
            .map(|i| format!("sentence {i}."))
            .collect::<Vec<_>>()
            .join(" ");
        let chunker = RecursiveChunker::new(ChunkerConfig {
            target_chars: 40,
            overlap_chars: 0,
            ..ChunkerConfig::default()
        })
        .expect("config");
        let chunks = chunker.split_documents(&[doc(&text)]).expect("split");
        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].next_id.as_deref(), Some(chunks[1].id.as_str()));
        assert_eq!(chunks[1].prev_id.as_deref(), Some(chunks[0].id.as_str()));
    }

    #[test]
    fn invalid_config_fails_fast() {
        assert!(RecursiveChunker::new(ChunkerConfig {
            target_chars: 10,
            overlap_chars: 10,
            ..ChunkerConfig::default()
        })
        .is_err());
    }

    #[test]
    fn markdown_preset_orders_heading_separators_first() {
        let config = ChunkerConfig::markdown();
        assert!(config.separators.iter().any(|s| s.contains("##")));
        let chunker = RecursiveChunker::new(config).expect("config");
        let chunks = chunker
            .split_documents(&[doc("# Title\n\nfirst body\n\n## Part\n\nsecond body")])
            .expect("split");
        assert!(!chunks.is_empty());
    }

    #[test]
    fn sentence_preset_prefers_sentence_boundaries() {
        let config = ChunkerConfig::sentence();
        assert_eq!(config.separators.first().map(String::as_str), Some(". "));
        let chunker = RecursiveChunker::new(config).expect("config");
        let chunks = chunker
            .split_documents(&[doc("first sentence. second sentence. third sentence")])
            .expect("split");
        assert!(!chunks.is_empty());
    }

    #[test]
    fn hierarchical_chunks_carry_parent_links() {
        let text = "first section body here\n\nsecond section body here";
        let chunker = RecursiveChunker::new(ChunkerConfig::default()).expect("config");
        let chunks = chunker.split_hierarchical(&[doc(text)]).expect("split");
        let parents: Vec<_> = chunks.iter().filter(|chunk| chunk.is_parent).collect();
        assert_eq!(parents.len(), 2);
        assert!(chunks
            .iter()
            .filter(|chunk| !chunk.is_parent)
            .all(|chunk| chunk.parent_id.is_some()));
    }

    #[test]
    fn code_preset_prefers_block_boundaries() {
        let config = ChunkerConfig::code();
        assert!(config.separators.iter().any(|s| s == "}\n"));
        let chunker = RecursiveChunker::new(config).expect("config");
        let chunks = chunker
            .split_documents(&[doc("fn a() {\nbody\n}\n\nfn b() {\nbody\n}")])
            .expect("split");
        assert!(!chunks.is_empty());
    }

    #[test]
    fn token_budget_caps_piece_sizes() {
        let text = (0..20)
            .map(|i| format!("word{i}"))
            .collect::<Vec<_>>()
            .join(" ");
        let chunker = RecursiveChunker::new(ChunkerConfig {
            token_budget: Some(5),
            ..ChunkerConfig::default()
        })
        .expect("config");
        let chunks = chunker.split_documents(&[doc(&text)]).expect("split");
        assert!(chunks.len() > 1);
        assert!(chunks.iter().all(|chunk| count_tokens(&chunk.content) <= 5));
    }

    #[test]
    fn markdown_preset_records_heading_path() {
        let chunker = RecursiveChunker::new(ChunkerConfig::markdown()).expect("config");
        let chunks = chunker
            .split_documents(&[doc("# Guide\n\n## Ownership\n\nRust ownership text")])
            .expect("split");
        assert!(chunks.iter().any(|chunk| {
            chunk.metadata.get("heading").map(String::as_str) == Some("Ownership")
                && chunk.metadata.get("heading_path").map(String::as_str)
                    == Some("Guide / Ownership")
        }));
    }

    #[test]
    fn markdown_preset_tracks_each_heading_path() {
        let chunker = RecursiveChunker::new(ChunkerConfig::markdown()).expect("config");
        let chunks = chunker
            .split_documents(&[doc(
                "# Guide\n\nintro\n\n## Ownership\n\nownership text\n\n## Borrowing\n\nborrowing text",
            )])
            .expect("split");
        let paths: Vec<_> = chunks
            .iter()
            .filter_map(|chunk| chunk.metadata.get("heading_path").cloned())
            .collect();
        assert!(paths.contains(&"Guide / Ownership".to_string()));
        assert!(paths.contains(&"Guide / Borrowing".to_string()));
        assert!(!paths
            .iter()
            .any(|path| path.contains("Borrowing") && path.contains("Ownership")));
    }

    #[test]
    fn table_preset_keeps_table_rows_together() {
        let table = "| name | value |\n| --- | --- |\n| alpha | one |\n| beta | two |";
        let chunker = RecursiveChunker::new(ChunkerConfig::table()).expect("config");
        let chunks = chunker.split_documents(&[doc(table)]).expect("split");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("| beta | two |"));
    }

    #[test]
    fn table_preset_keeps_long_tables_atomic() {
        let mut lines = vec!["| name | value |".to_string(), "| --- | --- |".to_string()];
        for index in 0..40 {
            lines.push(format!("| item-{index} | value-{index} |"));
        }
        let chunker = RecursiveChunker::new(ChunkerConfig {
            target_chars: 60,
            overlap_chars: 0,
            ..ChunkerConfig::table()
        })
        .expect("config");
        let chunks = chunker
            .split_documents(&[doc(&lines.join("\n"))])
            .expect("split");
        assert_eq!(chunks.len(), 1);
        assert!(chunks[0].content.contains("| item-39 | value-39 |"));
    }

    #[test]
    fn token_budget_rejects_zero() {
        assert!(RecursiveChunker::new(ChunkerConfig {
            token_budget: Some(0),
            ..ChunkerConfig::default()
        })
        .is_err());
        assert_eq!(count_tokens("  two  tokens "), 2);
    }
}
