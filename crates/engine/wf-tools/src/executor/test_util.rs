//! Shared test helpers for the executor test suites.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::mpsc;

/// Minimal mock HTTP server: accepts one connection, records the raw
/// request head, and responds with the given body/status.
pub(crate) fn mock_server(
    response_body: &'static str,
    status: &'static str,
) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = listener.local_addr().unwrap();
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut buf = Vec::new();
            let mut chunk = [0u8; 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.extend_from_slice(&chunk[..n]);
                        if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
            let head = String::from_utf8_lossy(&buf).to_string();
            let _ = tx.send(head);
            let resp = format!(
                "HTTP/1.1 {} \r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                status,
                response_body.len(),
                response_body
            );
            let _ = stream.write_all(resp.as_bytes());
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
    });
    (addr.to_string(), rx)
}
