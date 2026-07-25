pub type Timestamp = i64;

pub fn now() -> Timestamp {
    Utc::now().timestamp_millis()
}

pub fn timestamp_from_datetime(dt: &DateTime<Utc>) -> Timestamp {
    dt.timestamp_millis()
}

pub fn datetime_from_timestamp(ts: Timestamp) -> DateTime<Utc> {
    DateTime::from_timestamp_millis(ts)
        .unwrap_or_else(|| DateTime::from_timestamp(0, 0).unwrap())
}

pub fn timestamp_to_iso_string(ts: Timestamp) -> String {
    datetime_from_timestamp(ts).to_rfc3339()
}
