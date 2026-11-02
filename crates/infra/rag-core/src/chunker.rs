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
    /// Separators tried from coarse to fine.
    pub separators: Vec<String>,
    /// Separators defining top-level sections for hierarchical parents.
    pub parent_separators: Vec<String>,
    /// Whether document metadata is copied onto every chunk.
    pub keep_metadata: bool,
    /// Whether prev/next links are recorded between leaf chunks.
    pub keep_relations: bool,
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
                        metadata: self.metadata_for(document),
                        prev_id: None,
                        next_id: None,
                        parent_id: Some(parent_id.clone()),
                        is_parent: false,
                    });
                }
                self.link_range(&mut chunks, base);
                chunks.push(RagChunk {
                    id: parent_id,
                    document_id: document.id.clone(),
                    ordinal: base + leaves.len(),
                    content: text.to_string(),
                    metadata: self.metadata_for(document),
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
        let pieces = self.split_text(&document.content);
        let base_ordinal = 0;
        let mut chunks: Vec<RagChunk> = pieces
            .iter()
            .enumerate()
            .map(|(index, content)| RagChunk {
                id: chunk_id_for(&document.source, base_ordinal + index),
                document_id: document.id.clone(),
                ordinal: base_ordinal + index,
                content: content.clone(),
                metadata: self.metadata_for(document),
                prev_id: None,
                next_id: None,
                parent_id: None,
                is_parent: false,
            })
            .collect();
        self.link_range(&mut chunks, 0);
        chunks
    }

    fn metadata_for(&self, document: &RagDocument) -> HashMap<String, String> {
        if self.config.keep_metadata {
            document.metadata.clone()
        } else {
            HashMap::new()
        }
    }

    fn link_range(&self, chunks: &mut [RagChunk], base: usize) {
        if !self.config.keep_relations {
            return;
        }
        let len = chunks.len() - base;
        for offset in 0..len {
            let index = base + offset;
            if offset > 0 {
                let prev = chunks[index - 1].id.clone();
                chunks[index].prev_id = Some(prev);
            }
            if offset + 1 < len && !chunks[index].is_parent {
                let next = chunks[index + 1].id.clone();
                if !chunks[index + 1].is_parent {
                    chunks[index].next_id = Some(next);
                }
            }
        }
    }

    fn split_text(&self, text: &str) -> Vec<String> {
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
        apply_overlap(pieces, self.config.target_chars, self.config.overlap_chars)
    }
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
}
