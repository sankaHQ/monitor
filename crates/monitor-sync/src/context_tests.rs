use super::*;
use std::io::{Read, Write};

/// Real local HTTP boundary: account bearer, closed routes and exact preview bytes.
#[tokio::test]
async fn context_transport_keeps_preview_bytes_and_uses_account_routes() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let mut captured = Vec::new();
        for data in [
            serde_json::json!({"schemaVersions":[1],"policyVersion":1,"maxBytes":1048576}),
            serde_json::json!({"accepted":true}),
            serde_json::json!({"revoked":true}),
        ] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(3)))
                .unwrap();
            let mut request = Vec::new();
            let mut buf = [0; 4096];
            let head = loop {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
                if let Some(at) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    break at + 4;
                }
            };
            let headers = String::from_utf8_lossy(&request[..head]).to_ascii_lowercase();
            let length = headers
                .lines()
                .find_map(|l| l.strip_prefix("content-length: "))
                .map(|n| n.parse::<usize>().unwrap())
                .unwrap_or(0);
            while request.len() < head + length {
                let n = stream.read(&mut buf).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&buf[..n]);
            }
            captured.push(request);
            let body =
                serde_json::json!({"success":true,"data":data,"meta":{"ctx_id":"synthetic"}})
                    .to_string();
            write!(
                stream,
                "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                body.len()
            )
            .unwrap();
        }
        captured
    });
    let client = AgentClient::new(base, "synthetic").unwrap();
    let _: serde_json::Value = client
        .context_request("account-runtime", ContextRoute::Capabilities, None)
        .await
        .unwrap();
    let original = " {\"synthetic\": \"日本語\\n\"} ";
    let body = serde_json::json!({"grantId":uuid::Uuid::new_v4(),"packageJson":original,"sha256":"synthetic-hash"});
    let _: serde_json::Value = client
        .context_request("account-runtime", ContextRoute::Packages, Some(&body))
        .await
        .unwrap();
    let _: serde_json::Value = client
        .context_request("account-runtime", ContextRoute::Revoke, None)
        .await
        .unwrap();
    let requests = server.join().unwrap();
    for (request, method) in requests.iter().zip([
        "GET /api/v2/monitor/desktop/context/capabilities HTTP/1.1",
        "POST /api/v2/monitor/desktop/context/packages HTTP/1.1",
        "DELETE /api/v2/monitor/desktop/context/grants HTTP/1.1",
    ]) {
        let text = String::from_utf8_lossy(request);
        assert!(text.starts_with(method));
        assert!(text
            .to_ascii_lowercase()
            .contains("authorization: bearer account-runtime\r\n"));
    }
    let sent = requests[1]
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .unwrap()
        + 4;
    let parsed: serde_json::Value = serde_json::from_slice(&requests[1][sent..]).unwrap();
    assert_eq!(parsed, body);
    assert_eq!(parsed["packageJson"], original);
    assert!(requests[0].ends_with(b"\r\n\r\n"));
    assert!(requests[2].ends_with(b"\r\n\r\n"));
}
