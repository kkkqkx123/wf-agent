//! Conversation branch tree: the durable shape of one chat conversation.
//!
//! A chat session owns one tree. Every node is either one question/answer
//! `Turn` or a `Premise` (a pinned statement that only contributes input).
//! The tree references stored messages by id plus a content hash; it never
//! owns message bodies, which keeps the tree an atomic single-document write
//! and lets freshness be computed without loading a single message payload.

pub mod tree;

pub use tree::*;
