use wf_types::message::FileReadEntry;

use super::ConversationSession;

impl ConversationSession {
    /// Look up a file-read entry by path.
    pub fn file_read_entry(&self, path: &str) -> Option<&FileReadEntry> {
        self.state.compaction.file_reads.get(path)
    }

    /// Record a file-read entry.
    pub fn record_file_read(&mut self, entry: FileReadEntry) {
        self.state
            .compaction
            .file_reads
            .insert(entry.path.clone(), entry);
    }
}
