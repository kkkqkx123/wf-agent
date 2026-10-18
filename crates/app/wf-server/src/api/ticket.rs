//! Single-use connect tickets for browser credentialed streams.
//!
//! Browsers cannot attach custom headers to WebSocket or EventSource
//! requests, so the long-lived API key otherwise has to travel in the query
//! string where reverse proxies, CDN access logs and in-page scripts can read
//! it. A ticket is a short-lived, single-use, scope-bound token minted over a
//! normal authenticated REST call (key in the header) and consumed once by the
//! stream handler, so the persistent key never enters the stream URL.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::http::{HeaderMap, Uri};
use serde::Serialize;
use utoipa::ToSchema;

use crate::middleware::CorsConfig;

/// Query parameter carrying the ticket on a stream handshake.
pub(crate) const TICKET_QUERY: &str = "ticket";
/// Header a client must send to mint a ticket (mirrors the reference service).
pub(crate) const TICKET_HEADER: &str = "x-wf-ticket";
pub(crate) const TICKET_HEADER_VALUE: &str = "1";
/// Ticket lifetime in seconds.
pub(crate) const TICKET_TTL_SECS: i64 = 60;
/// Upper bound on live tickets so a minting loop cannot grow memory.
const TICKET_CAPACITY: usize = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TicketScope {
    Pty { id: String },
    Events,
}

#[derive(Debug, Clone, Serialize, ToSchema)]
pub(crate) struct StreamTicket {
    pub ticket: String,
    pub expires_in: i64,
}

struct TicketRecord {
    scope: TicketScope,
    expires_at: Instant,
}

#[derive(Default)]
struct TicketStore {
    tickets: HashMap<String, TicketRecord>,
}

fn store() -> &'static Mutex<TicketStore> {
    static STORE: OnceLock<Mutex<TicketStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(TicketStore::default()))
}

fn lock() -> std::sync::MutexGuard<'static, TicketStore> {
    match store().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

/// Drop expired tickets and, if the store is still full, evict the
/// earliest-expiring entry so minting never blocks behind a full map.
fn prune(store: &mut TicketStore) {
    let now = Instant::now();
    store.tickets.retain(|_, record| record.expires_at > now);
    while store.tickets.len() >= TICKET_CAPACITY {
        let victim = store
            .tickets
            .iter()
            .min_by_key(|(_, record)| record.expires_at)
            .map(|(ticket, _)| ticket.clone());
        match victim {
            Some(ticket) => {
                store.tickets.remove(&ticket);
            }
            None => break,
        }
    }
}

fn issue_with_ttl(scope: TicketScope, ttl: Duration) -> String {
    let ticket = wf_api::generate_id();
    let mut store = lock();
    prune(&mut store);
    store.tickets.insert(
        ticket.clone(),
        TicketRecord {
            scope,
            expires_at: Instant::now() + ttl,
        },
    );
    ticket
}

pub(crate) fn issue(scope: TicketScope) -> String {
    issue_with_ttl(scope, Duration::from_secs(TICKET_TTL_SECS as u64))
}

/// Atomically consume a ticket. Returns true only when the ticket exists, has
/// not expired and matches the requested scope.
pub(crate) fn consume(ticket: &str, scope: &TicketScope) -> bool {
    let mut store = lock();
    match store.tickets.remove(ticket) {
        Some(record) => record.expires_at > Instant::now() && &record.scope == scope,
        None => false,
    }
}

/// Whether a request is a stream handshake that presents a ticket instead of
/// the API key. The auth middleware uses this to defer validation to the
/// stream handler.
pub(crate) fn is_stream_ticket_request(uri: &Uri) -> bool {
    if crate::middleware::query_param(uri, TICKET_QUERY).is_none() {
        return false;
    }
    let path = uri.path();
    is_pty_connect_path(path) || path == "/api/v1/events/stream"
}

fn is_pty_connect_path(path: &str) -> bool {
    match path.strip_prefix("/api/v1/pty/") {
        Some(rest) => {
            let id = rest.strip_suffix("/connect").unwrap_or("");
            !id.is_empty() && !id.contains('/')
        }
        None => false,
    }
}

/// Shared guard for ticket minting endpoints: the client must present the
/// marker header and an origin allowed by the CORS policy.
pub(crate) fn ticket_request_authorized(headers: &HeaderMap, cors: &CorsConfig) -> bool {
    let marker = headers
        .get(TICKET_HEADER)
        .and_then(|value| value.to_str().ok());
    if marker != Some(TICKET_HEADER_VALUE) {
        return false;
    }
    let origin = headers
        .get(axum::http::header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    crate::middleware::origin_allowed(origin, cors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn issue_then_consume_once() {
        let ticket = issue_with_ttl(TicketScope::Events, Duration::from_secs(60));
        assert!(consume(&ticket, &TicketScope::Events));
        // Single use: the second consume must fail.
        assert!(!consume(&ticket, &TicketScope::Events));
    }

    #[test]
    fn consume_rejects_wrong_scope() {
        let ticket = issue_with_ttl(TicketScope::Events, Duration::from_secs(60));
        assert!(!consume(&ticket, &TicketScope::Pty { id: "pty-x".into() }));
        // The rejected consume still removes the ticket.
        assert!(!consume(&ticket, &TicketScope::Events));
    }

    #[test]
    fn consume_rejects_expired() {
        let ticket = issue_with_ttl(TicketScope::Events, Duration::from_millis(1));
        std::thread::sleep(Duration::from_millis(5));
        assert!(!consume(&ticket, &TicketScope::Events));
    }

    #[test]
    fn consume_rejects_unknown() {
        assert!(!consume("never-issued", &TicketScope::Events));
    }

    #[test]
    fn pty_connect_path_matching() {
        assert!(is_pty_connect_path("/api/v1/pty/pty-1/connect"));
        assert!(!is_pty_connect_path("/api/v1/pty/pty-1"));
        assert!(!is_pty_connect_path("/api/v1/pty/connect"));
        assert!(!is_pty_connect_path("/api/v1/other/pty-1/connect"));
    }
}
