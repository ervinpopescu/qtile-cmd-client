use crate::utils::ipc::Client;
use serde_json::json;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::thread;

#[test]
fn test_unframed_exchange() {
    let (mut server, client_stream) = UnixStream::pair().unwrap();

    let client_thread = thread::spawn(move || {
        // Replicating the unframed exchange logic from Client::send_request
        let mut stream = client_stream;
        let data = r#"{"hello": "world"}"#;
        stream.write_all(data.as_bytes()).unwrap();
        stream.shutdown(std::net::Shutdown::Write).unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    });

    // Server side
    let mut request = String::new();
    server.read_to_string(&mut request).unwrap();
    assert_eq!(request, r#"{"hello": "world"}"#);

    let response_data = r#"{"result": "ok"}"#;
    server.write_all(response_data.as_bytes()).unwrap();
    drop(server); // Close to send EOF

    let response = client_thread.join().unwrap();
    assert_eq!(response, r#"{"result": "ok"}"#);
}

#[test]
fn test_framed_exchange() {
    let (mut server, client_stream) = UnixStream::pair().unwrap();
    let mut client = Client::new_from_stream(client_stream, true);

    let client_thread = thread::spawn(move || {
        // Client::send_request logic for framed: wrap in envelope
        let data = r#"{"hello": "framed"}"#;
        let payload = json!({
            "message_type": "command",
            "content": serde_json::from_str::<serde_json::Value>(data).unwrap()
        });
        let framed_data = serde_json::to_string(&payload).unwrap();

        client.write_frame(framed_data.as_bytes()).unwrap();
        let resp = client.read_frame().unwrap();
        String::from_utf8(resp).unwrap()
    });

    // Server side: read header
    let mut header = [0u8; 4];
    server.read_exact(&mut header).unwrap();
    let len = u32::from_be_bytes(header);

    // Read body
    let mut body = vec![0u8; len as usize];
    server.read_exact(&mut body).unwrap();
    let body_str = String::from_utf8(body).unwrap();
    assert!(body_str.contains("\"message_type\":\"command\""));
    assert!(body_str.contains("\"content\":{\"hello\":\"framed\"}"));

    // Send enveloped response
    let resp_payload = json!({
        "message_type": "reply",
        "content": {"result": "framed_ok"}
    });
    let resp_body = serde_json::to_string(&resp_payload).unwrap();
    let resp_bytes = resp_body.as_bytes();
    let resp_len = resp_bytes.len() as u32;
    server.write_all(&resp_len.to_be_bytes()).unwrap();
    server.write_all(resp_bytes).unwrap();

    let response = client_thread.join().unwrap();
    assert_eq!(response, resp_body);
}

#[test]
fn test_read_frame_malformed_header() {
    let (mut server, client_stream) = UnixStream::pair().unwrap();
    let mut client = Client::new_from_stream(client_stream, true);

    // Server sends only 2 bytes of header then closes
    thread::spawn(move || {
        server.write_all(&[0, 0]).unwrap();
        drop(server);
    });

    let result = client.read_frame();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to read frame header"));
}

#[test]
fn test_read_frame_incomplete_body() {
    let (mut server, client_stream) = UnixStream::pair().unwrap();
    let mut client = Client::new_from_stream(client_stream, true);

    // Server sends header for 10 bytes, but only 5 bytes of body
    thread::spawn(move || {
        let len = 10u32;
        server.write_all(&len.to_be_bytes()).unwrap();
        server.write_all(b"12345").unwrap();
        drop(server);
    });

    let result = client.read_frame();
    assert!(result.is_err());
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("Failed to read frame body"));
}

#[test]
fn test_write_frame_disconnected() {
    let (server, client_stream) = UnixStream::pair().unwrap();
    let mut client = Client::new_from_stream(client_stream, true);

    // Close server immediately
    drop(server);

    let result = client.write_frame(b"hello");
    assert!(result.is_err());
}
