use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;
use std::thread;

use serde_json::Value;

fn serve_once(status: &str, response_body: &'static str) -> (String, thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind mock gateway");
    let address = listener.local_addr().expect("mock gateway address");
    let status = status.to_owned();
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept CLI request");
        let mut request = Vec::new();
        loop {
            let mut chunk = [0_u8; 4096];
            let read = stream.read(&mut chunk).expect("read CLI request");
            assert!(read > 0, "CLI closed before sending a complete request");
            request.extend_from_slice(&chunk[..read]);
            let Some(headers_end) = request.windows(4).position(|window| window == b"\r\n\r\n")
            else {
                continue;
            };
            let body_start = headers_end + 4;
            let headers = String::from_utf8_lossy(&request[..headers_end]).to_ascii_lowercase();
            let content_length = headers
                .lines()
                .find_map(|line| line.strip_prefix("content-length:"))
                .map(str::trim)
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if request.len() >= body_start + content_length {
                break;
            }
        }

        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{response_body}",
            response_body.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write mock gateway response");
        String::from_utf8(request).expect("CLI request is HTTP text plus JSON")
    });
    (format!("http://{address}"), handle)
}

#[test]
fn action_run_forwards_idempotency_key_and_emits_one_json_value() {
    let response = r#"{"id":"run-1","status":"queued"}"#;
    let (api_url, server) = serve_once("202 Accepted", response);
    let output = Command::new(env!("CARGO_BIN_EXE_clotho"))
        .args([
            "--api",
            &api_url,
            "--json",
            "actions",
            "run",
            "weave",
            "--actor",
            "automation",
            "--idempotency-key",
            "retry.action:01",
        ])
        .env_remove("CLOTHO_TOKEN")
        .output()
        .expect("run Clotho CLI");
    let request = server.join().expect("mock gateway thread");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let stdout: Value = serde_json::from_slice(&output.stdout).expect("one JSON stdout value");
    assert_eq!(stdout["id"], "run-1");

    let (headers, body) = request.split_once("\r\n\r\n").expect("HTTP request split");
    assert!(headers.starts_with("POST /api/v1/repos/weave/actions/runs HTTP/1.1"));
    assert!(headers
        .to_ascii_lowercase()
        .contains("idempotency-key: retry.action:01"));
    let body: Value = serde_json::from_str(body).expect("Action request JSON");
    assert_eq!(body["actor"], "automation");
    assert_eq!(body["workflow"], "ci");
}

#[test]
fn idempotency_conflict_uses_the_stable_conflict_exit_class() {
    let response = r#"{"version":"1","code":"idempotency_conflict","message":"key already used","request_id":"request-1","retryable":false}"#;
    let (api_url, server) = serve_once("409 Conflict", response);
    let output = Command::new(env!("CARGO_BIN_EXE_clotho"))
        .args([
            "--api",
            &api_url,
            "--json",
            "actions",
            "run",
            "weave",
            "--idempotency-key",
            "retry-01",
        ])
        .env_remove("CLOTHO_TOKEN")
        .output()
        .expect("run Clotho CLI");
    let _request = server.join().expect("mock gateway thread");

    assert_eq!(output.status.code(), Some(5));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("UTF-8 CLI error");
    assert!(stderr.contains("idempotency_conflict"));
    assert!(stderr.contains("request-1"));
}
