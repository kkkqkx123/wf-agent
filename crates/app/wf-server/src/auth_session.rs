// Password, Basic credentials and HMAC session tokens.
// Mirrors the reference server behavior without adding new dependencies:
// HMAC-SHA256 is built on the workspace sha2 crate, encodings on base64.

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use sha2::{Digest, Sha256};

/// Fixed username for password mode, matching the reference service.
pub const PASSWORD_USERNAME: &str = "opencode";
/// Session lifetime in seconds (30 days).
pub const SESSION_TTL_SECS: i64 = 30 * 24 * 60 * 60;
/// Cookie name prefix; the port is appended to keep local servers apart.
const SESSION_COOKIE_PREFIX: &str = "opencode_session";

/// Whether password protection is active.
pub fn password_required(password: &Option<String>) -> bool {
    password.as_ref().is_some_and(|value| !value.is_empty())
}

/// Constant-time equality for secrets.
fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        diff |= a ^ b;
    }
    diff == 0
}

fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; 32] {
    const BLOCK: usize = 64;
    let mut key_block = [0u8; BLOCK];
    if key.len() > BLOCK {
        let digest = Sha256::digest(key);
        key_block[..32].copy_from_slice(&digest);
    } else {
        key_block[..key.len()].copy_from_slice(key);
    }
    let mut inner_pad = [0x36u8; BLOCK];
    let mut outer_pad = [0x5cu8; BLOCK];
    for index in 0..BLOCK {
        inner_pad[index] ^= key_block[index];
        outer_pad[index] ^= key_block[index];
    }
    let mut inner = Sha256::new();
    inner.update(inner_pad);
    inner.update(message);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(outer_pad);
    outer.update(inner_digest);
    let digest = outer.finalize();
    let mut out = [0u8; 32];
    out.copy_from_slice(&digest);
    out
}

fn session_key(password: &str) -> [u8; 32] {
    hmac_sha256(password.as_bytes(), b"opencode-session-v1")
}

fn sign(password: &str, payload: &str) -> String {
    let key = session_key(password);
    let signature = hmac_sha256(&key, payload.as_bytes());
    URL_SAFE_NO_PAD.encode(signature)
}

/// Issue a session token of the form `<expires>.<signature>`.
pub fn issue_session(password: &str, now_ms: i64) -> String {
    let expires = now_ms / 1000 + SESSION_TTL_SECS;
    let payload = expires.to_string();
    format!("{payload}.{}", sign(password, &payload))
}

/// Verify a session token, checking expiry and signature.
pub fn verify_session(token: &str, password: &str, now_ms: i64) -> bool {
    let Some((expires_raw, signature)) = token.split_once('.') else {
        return false;
    };
    let Ok(expires) = expires_raw.parse::<i64>() else {
        return false;
    };
    if expires.saturating_mul(1000) <= now_ms {
        return false;
    }
    let expected = sign(password, expires_raw);
    constant_time_eq(expected.as_bytes(), signature.as_bytes())
}

/// Authorize Basic credentials against the configured password.
pub fn basic_authorized(username: &str, password: &str, expected: &str) -> bool {
    if username != PASSWORD_USERNAME {
        return false;
    }
    constant_time_eq(password.as_bytes(), expected.as_bytes())
}

/// Decode a Basic Authorization value into username and password.
pub fn decode_basic(value: &str) -> Option<(String, String)> {
    let encoded = value
        .strip_prefix("Basic ")
        .or_else(|| value.strip_prefix("basic "))?;
    let decoded = STANDARD.decode(encoded.trim()).ok()?;
    let text = String::from_utf8(decoded).ok()?;
    let (username, password) = text.split_once(':')?;
    Some((username.to_string(), password.to_string()))
}

/// Cookie name for a host value, carrying the port to keep local servers apart.
pub fn session_cookie_name(host: Option<&str>) -> String {
    let port = host.and_then(|value| {
        let without_scheme = value.rsplit("://").next().unwrap_or(value);
        let authority = without_scheme.split('/').next().unwrap_or(without_scheme);
        let authority = authority.rsplit('@').next().unwrap_or(authority);
        if authority.starts_with('[') {
            authority
                .rfind("]:")
                .map(|index| authority[index + 2..].to_string())
        } else if authority.matches(':').count() == 1 {
            authority.rsplit(':').next().map(str::to_string)
        } else {
            None
        }
    });
    match port {
        Some(port) if !port.is_empty() => format!("{SESSION_COOKIE_PREFIX}_{port}"),
        _ => SESSION_COOKIE_PREFIX.to_string(),
    }
}

/// Extract one cookie value from a Cookie header.
pub fn cookie_value(header: &str, name: &str) -> Option<String> {
    for part in header.split(';') {
        let part = part.trim();
        let Some((key, value)) = part.split_once('=') else {
            continue;
        };
        if key.trim() == name {
            return Some(value.trim().to_string());
        }
    }
    None
}

/// Whether the client should receive a Basic challenge.
/// Browser navigations get the challenge; fetch callers handle status codes.
pub fn wants_challenge(sec_fetch_mode: Option<&str>) -> bool {
    match sec_fetch_mode {
        None => true,
        Some(mode) => mode.trim().eq_ignore_ascii_case("navigate"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_round_trip() {
        let token = issue_session("secret", 1_000);
        assert!(verify_session(&token, "secret", 2_000));
        assert!(!verify_session(&token, "other", 2_000));
    }

    #[test]
    fn session_rejects_expiry() {
        let token = format!("100.{}", sign("secret", "100"));
        assert!(!verify_session(&token, "secret", 101_000));
    }

    #[test]
    fn basic_decode_round_trip() {
        let encoded = STANDARD.encode("opencode:hunter2");
        let decoded = decode_basic(&format!("Basic {encoded}")).unwrap();
        assert_eq!(decoded.0, "opencode");
        assert_eq!(decoded.1, "hunter2");
    }

    #[test]
    fn cookie_name_carries_port() {
        assert_eq!(
            session_cookie_name(Some("127.0.0.1:3000")),
            "opencode_session_3000"
        );
        assert_eq!(session_cookie_name(Some("example.com")), "opencode_session");
        assert_eq!(session_cookie_name(None), "opencode_session");
    }

    #[test]
    fn challenge_only_for_navigation() {
        assert!(wants_challenge(None));
        assert!(wants_challenge(Some("navigate")));
        assert!(!wants_challenge(Some("cors")));
    }
}
