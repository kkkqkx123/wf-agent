use crate::git_store::{TRAILER_ACTOR, TRAILER_SESSION, TRAILER_TOOL};

/// Parsed commit object.
#[derive(Debug, Clone)]
pub struct GitCommit {
    pub id: String,
    pub tree: String,
    pub parents: Vec<String>,
    pub author: String,
    pub committer: String,
    pub author_ts: i64,
    pub committer_ts: i64,
    pub message: String,
}

impl GitCommit {
    /// Value of the first trailer line with the given key, if any.
    pub fn trailer(&self, key: &str) -> Option<String> {
        trailers_of(&self.message)
            .into_iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    /// All values of a repeating trailer key, in order.
    pub fn trailers(&self, key: &str) -> Vec<String> {
        trailers_of(&self.message)
            .into_iter()
            .filter(|(k, _)| k == key)
            .map(|(_, v)| v)
            .collect()
    }
}

/// Parse `Key: value` trailers from the message tail.
pub(crate) fn trailers_of(message: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in message.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim();
            if key.is_empty() || key.contains(' ') {
                break;
            }
            out.push((key.to_string(), v.trim().to_string()));
        } else {
            break;
        }
    }
    out.reverse();
    out
}

/// Split a free-form actor string into a valid name/email pair. Inputs that
/// already carry an email keep it, bare names get a local placeholder.
fn split_actor(raw: &str) -> (String, String) {
    let trimmed = raw.trim();
    if let Some(start) = trimmed.find('<') {
        if let Some(end) = trimmed.find('>') {
            if start < end {
                let name = trimmed[..start].trim();
                let email = trimmed[start + 1..end].trim();
                if !email.is_empty() {
                    return (
                        if name.is_empty() {
                            "checkpoint".to_string()
                        } else {
                            name.to_string()
                        },
                        email.to_string(),
                    );
                }
            }
        }
    }
    if trimmed.is_empty() {
        ("checkpoint".to_string(), "local".to_string())
    } else {
        (trimmed.to_string(), "local".to_string())
    }
}

pub(super) fn signature_for(raw: &str, secs: i64) -> gix_actor::Signature {
    let (name, email) = split_actor(raw);
    gix_actor::Signature {
        name: name.as_bytes().to_vec().into(),
        email: email.as_bytes().to_vec().into(),
        time: gix_date::Time::new(secs, 0),
    }
}

pub(super) fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Build a commit message from an intent line plus trailers.
pub fn commit_message(
    intent: &str,
    actor: Option<&str>,
    session: Option<&str>,
    tool: Option<&str>,
    extra_trailers: &[(String, String)],
) -> String {
    let mut message = intent.trim().to_string();
    if message.is_empty() {
        message = "checkpoint".to_string();
    }
    if let Some(actor) = actor {
        message.push_str(&format!("\n{TRAILER_ACTOR}: {actor}"));
    }
    if let Some(session) = session.filter(|s| !s.is_empty()) {
        message.push_str(&format!("\n{TRAILER_SESSION}: {session}"));
    }
    if let Some(tool) = tool.filter(|t| !t.is_empty()) {
        message.push_str(&format!("\n{TRAILER_TOOL}: {tool}"));
    }
    for (k, v) in extra_trailers {
        message.push_str(&format!("\n{k}: {v}"));
    }
    message
}
