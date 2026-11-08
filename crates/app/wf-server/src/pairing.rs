// Single-use pairing codes for browser sign-in.
// A code is issued over an authenticated call and consumed once by the
// pairing connect endpoint, which then issues a session token.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long an issued code stays consumable.
const CODE_TTL: Duration = Duration::from_secs(5 * 60);
/// Upper bound on live codes.
const CODE_CAPACITY: usize = 1_000;

struct CodeRecord {
    expires_at: Instant,
}

#[derive(Default)]
struct CodeStore {
    codes: HashMap<String, CodeRecord>,
}

fn store() -> &'static Mutex<CodeStore> {
    static STORE: OnceLock<Mutex<CodeStore>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(CodeStore::default()))
}

fn prune(store: &mut CodeStore) {
    let now = Instant::now();
    store.codes.retain(|_, record| record.expires_at > now);
    while store.codes.len() >= CODE_CAPACITY {
        let victim = store
            .codes
            .iter()
            .min_by_key(|(_, record)| record.expires_at)
            .map(|(code, _)| code.clone());
        match victim {
            Some(code) => {
                store.codes.remove(&code);
            }
            None => break,
        }
    }
}

/// Issue a one-time pairing code with a five minute lifetime.
pub fn issue() -> (String, i64) {
    let code = wf_common::generate_id().replace('-', "");
    let mut store = store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    prune(&mut store);
    store.codes.insert(
        code.clone(),
        CodeRecord {
            expires_at: Instant::now() + CODE_TTL,
        },
    );
    (code, CODE_TTL.as_secs() as i64)
}

/// Consume a code exactly once.
pub fn consume(code: &str) -> bool {
    let mut store = store()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match store.codes.remove(code.trim()) {
        Some(record) => record.expires_at > Instant::now(),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_consumes_once() {
        let (code, expires_in) = issue();
        assert!(expires_in > 0);
        assert!(consume(&code));
        assert!(!consume(&code));
    }

    #[test]
    fn unknown_code_rejected() {
        assert!(!consume("missing-code"));
    }
}
