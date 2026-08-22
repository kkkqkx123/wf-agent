use chrono::{DateTime, Utc};

pub use wf_types::Timestamp;

pub fn now() -> Timestamp {
    Utc::now().timestamp_millis()
}

pub fn datetime_from_timestamp(ts: Timestamp) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ts).unwrap_or(DateTime::UNIX_EPOCH)
}

pub fn timestamp_to_iso(ts: Timestamp) -> String {
    datetime_from_timestamp(ts).to_rfc3339()
}
