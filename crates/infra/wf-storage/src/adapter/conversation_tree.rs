use crate::adapter::base::BaseStorageAdapter;
use crate::domain::store::{FilterOp, QueryFilter};
use crate::error::StorageError;
use std::future::Future;

/// Read options for conversation trees.
///
/// A session owns one `Session` tree plus zero or more `Graft` trees, so the
/// only filters any read path needs are the session anchor and the kind.
#[derive(Debug, Clone, Default)]
pub struct ConversationTreeListOptions {
    pub offset: Option<u64>,
    pub limit: Option<u64>,
    pub session_id_filter: Option<String>,
}

impl From<ConversationTreeListOptions> for QueryFilter {
    fn from(opts: ConversationTreeListOptions) -> Self {
        let mut filter = QueryFilter::new();
        if let Some(offset) = opts.offset {
            filter.add_op(FilterOp::Offset(offset));
        }
        if let Some(limit) = opts.limit {
            filter.add_op(FilterOp::Limit(limit));
        }
        if let Some(value) = opts.session_id_filter {
            filter.add_op(FilterOp::Eq("sessionId".into(), value));
        }
        filter
    }
}

pub trait ConversationTreeStorageAdapter:
    BaseStorageAdapter<wf_types::ConversationTree, ConversationTreeListOptions>
{
    /// Every tree of one session: the main tree plus whatever sits in its pool.
    fn list_by_session<'a>(
        &'a self,
        session_id: &'a str,
    ) -> impl Future<Output = Result<Vec<wf_types::ConversationTree>, StorageError>> + Send + 'a;

    /// The session's main tree, if it has one yet.
    fn load_session_tree<'a>(
        &'a self,
        session_id: &'a str,
    ) -> impl Future<Output = Result<Option<wf_types::ConversationTree>, StorageError>> + Send + 'a;
}
