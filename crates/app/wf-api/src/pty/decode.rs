pub fn incomplete_tail_len(bytes: &[u8]) -> usize {
    let window = bytes.len().min(4);
    for len in 1..=window {
        let start = bytes.len() - len;
        let tail = &bytes[start..];
        if std::str::from_utf8(tail).is_ok() {
            continue;
        }
        if std::str::from_utf8(tail).unwrap_err().error_len().is_none() {
            return len;
        }
    }
    0
}

pub fn decode_complete(prefix: &mut Vec<u8>) -> String {
    if prefix.is_empty() {
        return String::new();
    }
    let tail = incomplete_tail_len(prefix);
    if tail == 0 {
        let text = String::from_utf8_lossy(prefix).to_string();
        prefix.clear();
        text
    } else {
        let split = prefix.len() - tail;
        let text = String::from_utf8_lossy(&prefix[..split]).to_string();
        prefix.drain(..split);
        text
    }
}