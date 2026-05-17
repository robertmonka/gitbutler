use super::GiteaClient;
use std::io::{ErrorKind, Read as _, Write as _};
use std::net::TcpListener;
use std::time::{Duration, Instant};

struct MockResponse {
    path: &'static str,
    status: reqwest::StatusCode,
    body: &'static str,
}

fn mock_client(responses: Vec<MockResponse>) -> (GiteaClient, std::thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let addr = listener.local_addr().unwrap();
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
            while !request.windows(4).any(|window| window == b"\r\n\r\n") {
                let read = stream.read(&mut chunk).unwrap();
                assert_ne!(read, 0, "request should include complete HTTP headers");
                request.extend_from_slice(&chunk[..read]);
            }
            let request = String::from_utf8(request).unwrap();
            let mut request_line = request.lines().next().unwrap().split_whitespace();
            assert_eq!(
                request_line.next(),
                Some("GET"),
                "checks client uses GET requests"
            );
            assert_eq!(
                request_line.next(),
                Some(expected.path),
                "checks client requests the expected endpoint"
            );

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
    (GiteaClient::new_for_tests(format!("http://{addr}")), server)
}

#[tokio::test(flavor = "current_thread")]
async fn slash_branch_resolves_via_commits_query_then_sha_statuses() {
    let sha = "0123456789abcdef0123456789abcdef01234567";
    let (client, server) = mock_client(vec![
        MockResponse {
            path: "/repos/optima/optima-api/commits?sha=fix%2Fksef-invoice-candidates-build&limit=1",
            status: reqwest::StatusCode::OK,
            body: r#"[{"sha":"0123456789abcdef0123456789abcdef01234567"}]"#,
        },
        MockResponse {
            path: "/repos/optima/optima-api/commits/0123456789abcdef0123456789abcdef01234567/statuses",
            status: reqwest::StatusCode::OK,
            body: r#"[{"id":1,"context":"ci","status":"success"}]"#,
        },
    ]);

    let checks = client
        .list_checks_for_ref("optima", "optima-api", "fix/ksef-invoice-candidates-build")
        .await
        .expect("a slash branch should resolve through the commits query")
        .expect("a resolved slash branch is authoritative");
    assert_eq!(checks.len(), 1);
    assert_eq!(checks[0].head_sha, sha);
    assert_eq!(checks[0].context, "ci");
    server.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn hex_sha_skips_commit_resolution() {
    let (client, server) = mock_client(vec![MockResponse {
        path: "/repos/optima/optima-api/commits/0123456789abcdef0123456789abcdef01234567/statuses",
        status: reqwest::StatusCode::OK,
        body: r#"[]"#,
    }]);

    let checks = client
        .list_checks_for_ref(
            "optima",
            "optima-api",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .await
        .expect("a commit SHA should query statuses directly")
        .expect("an existing SHA without statuses is authoritative");
    assert!(
        checks.is_empty(),
        "an existing SHA without statuses has no checks"
    );
    server.join().unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn slash_branch_400_is_an_unresolvable_ref() {
    let (client, server) = mock_client(vec![MockResponse {
        path: "/repos/optima/optima-api/commits?sha=fix%2Fksef-invoice-candidates-build&limit=1",
        status: reqwest::StatusCode::BAD_REQUEST,
        body: "",
    }]);

    let checks = client
        .list_checks_for_ref("optima", "optima-api", "fix/ksef-invoice-candidates-build")
        .await
        .expect("a 400 from Gitea for a slash branch is not an API failure");
    assert!(
        checks.is_none(),
        "an unresolvable slash branch must not replace cached checks"
    );
    server.join().unwrap();
}
