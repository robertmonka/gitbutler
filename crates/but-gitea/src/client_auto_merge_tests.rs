use super::{GiteaClient, SetPullRequestAutoMergeParams};
use std::io::{ErrorKind, Read as _, Write as _};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct CapturedRequest {
    method: String,
    body: String,
}

struct MockResponse {
    method: &'static str,
    path: &'static str,
    status: reqwest::StatusCode,
    body: &'static str,
}

fn mock_client(
    responses: Vec<MockResponse>,
) -> (
    GiteaClient,
    Arc<Mutex<Vec<CapturedRequest>>>,
    std::thread::JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let captured_for_server = Arc::clone(&captured);
    let server = std::thread::spawn(move || {
        for expected in responses {
            let deadline = Instant::now() + Duration::from_secs(2);
            let mut stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,
                    Err(err)
                        if err.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(5));
                    }
                    Err(err) => panic!("expected request to {}: {err}", expected.path),
                }
            };
            stream.set_nonblocking(false).unwrap();

            let mut request = Vec::new();
            let mut chunk = [0; 1024];
            loop {
                let read = stream.read(&mut chunk).unwrap();
                assert_ne!(read, 0, "request should include complete HTTP headers");
                request.extend_from_slice(&chunk[..read]);
                if !request.windows(4).any(|window| window == b"\r\n\r\n") {
                    continue;
                }
                let header_end = request
                    .windows(4)
                    .position(|window| window == b"\r\n\r\n")
                    .unwrap()
                    + 4;
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.strip_prefix("Content-Length:")
                            .or_else(|| line.strip_prefix("content-length:"))
                            .map(|value| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                while request.len() < header_end + content_length {
                    let read = stream.read(&mut chunk).unwrap();
                    assert_ne!(read, 0, "request body should arrive in full");
                    request.extend_from_slice(&chunk[..read]);
                }
                break;
            }

            let header_end = request
                .windows(4)
                .position(|window| window == b"\r\n\r\n")
                .unwrap()
                + 4;
            let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
            let body = String::from_utf8(request[header_end..].to_vec()).unwrap();
            let mut request_line = headers.lines().next().unwrap().split_whitespace();
            let method = request_line.next().unwrap().to_owned();
            let path = request_line.next().unwrap().to_owned();
            assert_eq!(method, expected.method, "unexpected HTTP method");
            assert_eq!(path, expected.path, "unexpected request path");
            captured_for_server
                .lock()
                .unwrap()
                .push(CapturedRequest { method, body });

            let reason = expected.status.canonical_reason().unwrap_or("Unknown");
            write!(
                stream,
                "HTTP/1.1 {} {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                expected.status.as_u16(),
                reason,
                expected.body.len(),
                expected.body
            )
            .unwrap();
        }
    });
    (
        GiteaClient::new_for_tests(format!("http://{addr}")),
        captured,
        server,
    )
}

#[tokio::test(flavor = "current_thread")]
async fn enabling_auto_merge_posts_merge_when_checks_succeed() {
    let (client, captured, server) = mock_client(vec![MockResponse {
        method: "POST",
        path: "/repos/acme/widget/pulls/42/merge",
        status: reqwest::StatusCode::OK,
        body: "{}",
    }]);

    client
        .set_pull_request_auto_merge(&SetPullRequestAutoMergeParams {
            owner: "acme",
            repo: "widget",
            pr_number: 42,
            enabled: true,
            merge_method: None,
        })
        .await
        .expect("enable auto-merge should succeed");

    server.join().unwrap();
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert!(
        requests[0].body.contains(r#""Do":"merge""#)
            || requests[0].body.contains(r#""Do": "merge""#),
        "expected merge method Do=merge, got {}",
        requests[0].body
    );
    assert!(
        requests[0]
            .body
            .contains(r#""merge_when_checks_succeed":true"#)
            || requests[0]
                .body
                .contains(r#""merge_when_checks_succeed": true"#),
        "expected merge_when_checks_succeed=true, got {}",
        requests[0].body
    );
}

#[tokio::test(flavor = "current_thread")]
async fn enabling_auto_merge_honors_explicit_merge_method() {
    let (client, captured, server) = mock_client(vec![MockResponse {
        method: "POST",
        path: "/repos/acme/widget/pulls/7/merge",
        status: reqwest::StatusCode::OK,
        body: "{}",
    }]);

    client
        .set_pull_request_auto_merge(&SetPullRequestAutoMergeParams {
            owner: "acme",
            repo: "widget",
            pr_number: 7,
            enabled: true,
            merge_method: Some("squash"),
        })
        .await
        .expect("enable auto-merge should succeed");

    server.join().unwrap();
    let requests = captured.lock().unwrap();
    assert!(
        requests[0].body.contains(r#""Do":"squash""#)
            || requests[0].body.contains(r#""Do": "squash""#),
        "expected merge method Do=squash, got {}",
        requests[0].body
    );
    assert!(
        requests[0]
            .body
            .contains(r#""merge_when_checks_succeed":true"#)
            || requests[0]
                .body
                .contains(r#""merge_when_checks_succeed": true"#),
        "expected merge_when_checks_succeed=true, got {}",
        requests[0].body
    );
}

#[tokio::test(flavor = "current_thread")]
async fn disabling_auto_merge_deletes_scheduled_merge() {
    let (client, captured, server) = mock_client(vec![MockResponse {
        method: "DELETE",
        path: "/repos/acme/widget/pulls/42/merge",
        status: reqwest::StatusCode::OK,
        body: "",
    }]);

    client
        .set_pull_request_auto_merge(&SetPullRequestAutoMergeParams {
            owner: "acme",
            repo: "widget",
            pr_number: 42,
            enabled: false,
            merge_method: None,
        })
        .await
        .expect("disable auto-merge should succeed");

    server.join().unwrap();
    let requests = captured.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].method, "DELETE");
    assert!(requests[0].body.is_empty());
}
