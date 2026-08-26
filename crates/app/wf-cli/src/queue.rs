//! Prompt queue for the mini session: one turn runs at a time, submits
//! while a turn is active are queued and drained in order (the opencode
//! `runPromptQueue` serial semantics).
//!
//! The queue is pure data: push / pop / remove / edit are the whole
//! surface, the queued panel renders from [`PromptQueue::items`] and the
//! mini event loop owns the drain policy (`pop` after a turn terminal
//! event).

/// One queued prompt (monotonic id + sanitized text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedPrompt {
    pub id: u64,
    pub text: String,
}

/// FIFO queue of prompts waiting for the active turn to finish.
#[derive(Debug, Clone, Default)]
pub struct PromptQueue {
    next_id: u64,
    items: Vec<QueuedPrompt>,
}

impl PromptQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Queue a prompt; empty text is ignored.
    pub fn push(&mut self, text: impl Into<String>) -> Option<QueuedPrompt> {
        let text = text.into();
        if text.trim().is_empty() {
            return None;
        }
        self.next_id += 1;
        let prompt = QueuedPrompt {
            id: self.next_id,
            text,
        };
        self.items.push(prompt.clone());
        Some(prompt)
    }

    /// Take the oldest queued prompt.
    pub fn pop(&mut self) -> Option<QueuedPrompt> {
        if self.items.is_empty() {
            None
        } else {
            Some(self.items.remove(0))
        }
    }

    /// Remove a queued prompt by id (queued panel Delete).
    pub fn remove(&mut self, id: u64) -> Option<QueuedPrompt> {
        let pos = self.items.iter().position(|p| p.id == id)?;
        Some(self.items.remove(pos))
    }

    /// Take a queued prompt by id for editing (queued panel Enter): the
    /// prompt leaves the queue and the text is handed back to the composer.
    pub fn take_for_edit(&mut self, id: u64) -> Option<QueuedPrompt> {
        self.remove(id)
    }

    /// Queued prompts in order.
    pub fn items(&self) -> &[QueuedPrompt] {
        &self.items
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Drop every queued prompt.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pushes_and_pops_in_fifo_order() {
        let mut q = PromptQueue::new();
        q.push("first");
        q.push("second");
        assert_eq!(q.len(), 2);
        assert_eq!(q.pop().map(|p| p.text), Some("first".to_string()));
        assert_eq!(q.pop().map(|p| p.text), Some("second".to_string()));
        assert!(q.pop().is_none());
        assert!(q.is_empty());
    }

    #[test]
    fn empty_pushes_are_ignored() {
        let mut q = PromptQueue::new();
        assert!(q.push("   ").is_none());
        assert!(q.push("").is_none());
        assert!(q.is_empty());
    }

    #[test]
    fn ids_are_monotonic_and_unique() {
        let mut q = PromptQueue::new();
        q.push("a");
        q.push("b");
        let ids: Vec<u64> = q.items().iter().map(|p| p.id).collect();
        assert_eq!(ids.len(), 2);
        assert_ne!(ids[0], ids[1]);
        // A popped id is never reused.
        let popped = q.pop().unwrap();
        q.push("c");
        assert_ne!(q.items()[0].id, popped.id);
    }

    #[test]
    fn remove_and_edit_by_id() {
        let mut q = PromptQueue::new();
        let a = q.push("a").unwrap();
        q.push("b");
        assert_eq!(q.len(), 2);

        let edited = q.take_for_edit(a.id).unwrap();
        assert_eq!(edited.text, "a");
        assert_eq!(q.len(), 1);

        let missing = q.remove(999);
        assert!(missing.is_none());
        assert_eq!(q.len(), 1);

        q.clear();
        assert!(q.is_empty());
    }
}
