use std::{
    io::{self, Read, Write},
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{bail, Context};
use serde_json::{json, Value};
use shellexpand::tilde;

/// Per-syscall read timeout used with `set_read_timeout`. Kept short so that
/// the manual read loop can poll the overall `READ_TOTAL_TIMEOUT` deadline.
const READ_SYSCALL_TIMEOUT: Duration = Duration::from_secs(1);

/// Overall deadline for reading the full response. Requests that produce large
/// responses (e.g. `eval`, `doc`) may need longer than the legacy 5-second cap.
const READ_TOTAL_TIMEOUT: Duration = Duration::from_secs(10);

/// Timeout applied to `write_all` so that a hung Qtile process cannot stall
/// `qticc` indefinitely.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Discovers the Qtile socket file path by checking the following in order:
/// 1.  An explicitly provided display name.
/// 2.  `WAYLAND_DISPLAY` environment variable.
/// 3.  `DISPLAY` environment variable.
/// 4.  Default locations for Wayland (`wayland-0`) and X11 (`:0`).
pub(crate) fn find_sockfile(display: Option<String>) -> PathBuf {
    let xdg_cache_home = std::env::var("XDG_CACHE_HOME").unwrap_or(tilde("~/.cache").to_string());
    let wayland_display = std::env::var("WAYLAND_DISPLAY").ok();
    let x11_display = std::env::var("DISPLAY").ok();

    find_sockfile_with_env(display, Some(xdg_cache_home), wayland_display, x11_display)
}

pub(crate) fn find_sockfile_with_env(
    display: Option<String>,
    xdg_cache_home: Option<String>,
    wayland_display: Option<String>,
    x11_display: Option<String>,
) -> PathBuf {
    let xdg_cache_home = xdg_cache_home.unwrap_or_else(|| tilde("~/.cache").to_string());
    let cache_dir = Path::new(&xdg_cache_home);

    match display {
        Some(s) => cache_dir.join("qtile").join(format!("qtilesocket.{s}")),
        None => {
            if let Some(s) = wayland_display {
                return cache_dir.join("qtile").join(format!("qtilesocket.{s}"));
            }
            if let Some(s) = x11_display {
                return cache_dir.join("qtile").join(format!("qtilesocket.{s}"));
            }

            for default_display in ["wayland-0", ":0", ":99"] {
                let sockfile = cache_dir
                    .join("qtile")
                    .join(format!("qtilesocket.{default_display}"));
                if std::path::Path::exists(&sockfile) {
                    return sockfile;
                }
            }
            // Fallback to wayland-0 if nothing found
            cache_dir.join("qtile").join("qtilesocket.wayland-0")
        }
    }
}

/// Client for communicating with the Qtile IPC socket.
pub struct Client {
    stream: UnixStream,
}

impl Client {
    #[cfg(feature = "framing")]
    pub fn new_from_stream(stream: UnixStream, _framed: bool) -> Self {
        Self { stream }
    }

    pub fn connect() -> anyhow::Result<Self> {
        Self::connect_with_path(None)
    }

    pub fn connect_with_path(path: Option<PathBuf>) -> anyhow::Result<Self> {
        let sockfile = path.unwrap_or_else(|| find_sockfile(None));
        let stream = UnixStream::connect(&sockfile)
            .with_context(|| format!("could not connect to socket: {sockfile:?}"))?;
        // Issue 4 / Issue 1: use named constants; set both read and write timeouts.
        // The read timeout is per-syscall; the manual read loop enforces the total deadline.
        stream
            .set_read_timeout(Some(READ_SYSCALL_TIMEOUT))
            .context("Could not set read timeout on the stream")?;
        stream
            .set_write_timeout(Some(WRITE_TIMEOUT))
            .context("Could not set write timeout on the stream")?;
        Ok(Self { stream })
    }

    /// Send a message and get a response using a one-off connection.
    /// When `framed` is true, uses the JSON IPC length-prefixed framing protocol.
    /// Falls back to legacy unframed mode otherwise, with auto-retry on empty response.
    pub fn send_request(data: String, framed: bool) -> anyhow::Result<String> {
        let res = Self::try_send_request(data.clone(), framed);
        // If unframed returned empty, retry with framing (server may have dropped legacy support).
        if !framed {
            if let Ok(ref s) = res {
                if s.is_empty() {
                    return Self::try_send_request(data, true);
                }
            }
            return res;
        }
        // If framing failed at the transport level (I/O error), the server likely uses the
        // legacy protocol (no framing support). Retry without framing.
        if let Err(ref e) = res {
            let is_io = e
                .chain()
                .any(|cause| cause.downcast_ref::<io::Error>().is_some());
            if is_io {
                return Self::try_send_request(data, false);
            }
        }
        res
    }

    fn try_send_request(data: String, framed: bool) -> anyhow::Result<String> {
        let mut client = Self::connect()?;
        if framed {
            #[cfg(feature = "framing")]
            {
                let payload = serde_json::json!({
                    "message_type": "command",
                    "content": serde_json::from_str::<serde_json::Value>(&data)
                        .unwrap_or(serde_json::Value::String(data.clone()))
                });
                let framed_data = serde_json::to_string(&payload)?;
                client.write_frame(framed_data.as_bytes())?;
                let bytes = client.read_frame()?;
                return String::from_utf8(bytes).context("frame payload is not valid UTF-8");
            }
            #[cfg(not(feature = "framing"))]
            {
                let _ = framed; // suppress unused warning
            }
        }
        client.stream.write_all(data.as_bytes())?;

        // Issue 2: ENOTCONN / BrokenPipe from shutdown means the server already closed
        // its end — the write was already received, so this is harmless. Only propagate
        // unexpected errors.
        match client.stream.shutdown(std::net::Shutdown::Write) {
            Ok(()) => {}
            Err(e)
                if e.kind() == io::ErrorKind::NotConnected
                    || e.kind() == io::ErrorKind::BrokenPipe =>
            {
                // Server closed first; the payload was already delivered.
            }
            Err(e) => return Err(e).context("Could not shutdown writing on the stream"),
        }

        // Issue 3 / Issue 6: use a manual read loop into Vec<u8> so that a per-syscall
        // WouldBlock/TimedOut does not discard buffered bytes, and so that non-UTF-8
        // responses produce a clear error instead of an opaque io::Error.
        let mut buf = [0u8; 4096];
        let mut raw: Vec<u8> = Vec::new();
        let deadline = Instant::now() + READ_TOTAL_TIMEOUT;
        loop {
            match client.stream.read(&mut buf) {
                Ok(0) => break, // EOF
                Ok(n) => raw.extend_from_slice(&buf[..n]),
                Err(e)
                    if e.kind() == io::ErrorKind::WouldBlock
                        || e.kind() == io::ErrorKind::TimedOut =>
                {
                    if !raw.is_empty() {
                        // We have data and the timeout fired — treat as EOF.
                        break;
                    }
                    if Instant::now() >= deadline {
                        bail!("ipc: timed out waiting for response after {READ_TOTAL_TIMEOUT:?}");
                    }
                    // No data yet; keep waiting.
                }
                Err(e) => return Err(e).context("ipc: error reading response"),
            }
        }

        // Issue 6: explicit UTF-8 validation with a clear error message.
        String::from_utf8(raw).context("ipc: response is not valid UTF-8")
    }

    /// Write a length-prefixed frame (4-byte big-endian length + payload).
    #[cfg(feature = "framing")]
    pub fn write_frame(&mut self, data: &[u8]) -> anyhow::Result<()> {
        let len = data.len() as u32;
        self.stream
            .write_all(&len.to_be_bytes())
            .context("Failed to write frame header")?;
        self.stream
            .write_all(data)
            .context("Failed to write frame body")?;
        Ok(())
    }

    /// Read a length-prefixed frame (4-byte big-endian length + payload).
    #[cfg(feature = "framing")]
    pub fn read_frame(&mut self) -> anyhow::Result<Vec<u8>> {
        let mut len_buf = [0u8; 4];
        self.stream
            .read_exact(&mut len_buf)
            .context("Failed to read frame header")?;
        let len = u32::from_be_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        self.stream
            .read_exact(&mut buf)
            .context("Failed to read frame body")?;
        Ok(buf)
    }

    /// Legacy wrapper for backward compatibility.
    #[allow(dead_code)]
    pub fn send(data: String) -> anyhow::Result<String> {
        Self::send_request(data, false)
    }

    /// Validates and parses the Qtile IPC response.
    pub fn match_response(
        response: anyhow::Result<String>,
        _framed: bool,
    ) -> anyhow::Result<Value> {
        let response = response?;
        let mut s: Value = serde_json::from_str(&response).context("ipc.Client: invalid JSON")?;

        // Handle enveloped message if present (new JSON IPC).
        // Issue 5: if message_type is present but not "reply", surface it explicitly
        // rather than falling through to the status/result extraction which will fail
        // with a confusing "missing 'status'" error.
        if let Value::Object(ref mut map) = s {
            if map.contains_key("message_type") {
                if map.get("message_type") == Some(&json!("reply")) {
                    if let Some(content) = map.remove("content") {
                        s = content;
                    }
                } else {
                    let msg_type = map
                        .get("message_type")
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    let content = map
                        .get("content")
                        .map(|v| v.to_string())
                        .unwrap_or_default();
                    bail!(
                        "ipc.Client: unexpected envelope message_type={msg_type} content={content}"
                    );
                }
            }
        }

        let s = Self::decode_qtile_tuples(s);
        match s {
            // Modern JSON response format
            Value::Object(map) => {
                // Qtile may return {"error": "..."} directly (e.g. "Session locked.")
                if let Some(Value::String(e)) = map.get("error") {
                    bail!("{e}");
                }

                let status = map
                    .get("status")
                    .context("ipc.Client: missing 'status' in reply content")?;
                let result = map
                    .get("result")
                    .or_else(|| map.get("data"))
                    .unwrap_or(&Value::Null);

                match status {
                    Value::Number(n) => {
                        let n = n.as_u64().context("ipc.Client: invalid status code")?;
                        match n {
                            0 => Ok(result.clone()),
                            n => bail!("{}", Self::format_error_result(result, n)),
                        }
                    }
                    _ => bail!("ipc.Client: status is not a number: {status:?}"),
                }
            }
            // Legacy tuple-based response format
            Value::Array(array) => {
                if array.is_empty() {
                    bail!("ipc.Client: received empty response array");
                }
                let status = &array[0];
                let result = if array.len() > 1 {
                    &array[1]
                } else {
                    &Value::Null
                };

                match status {
                    Value::Number(n) => {
                        let n = n.as_u64().context("ipc.Client: invalid status code")?;
                        match n {
                            0 => Ok(result.clone()),
                            n => bail!("{}", Self::format_error_result(result, n)),
                        }
                    }
                    _ => bail!("ipc.Client: status is not a number: {status:?}"),
                }
            }
            _ => bail!("ipc.Client: bad response by qtile!? (Unknown top-level: {s:?})"),
        }
    }

    fn format_error_result(result: &Value, code: u64) -> String {
        match result {
            Value::String(s) => format!("ipc.Client: response_code = {code}:\n{s}"),
            Value::Array(arr) => {
                // Issue 7: preserve non-string values (numbers, booleans, objects, null)
                // by falling back to JSON rendering instead of silently discarding them.
                let msg = arr
                    .iter()
                    .map(|v| {
                        v.as_str()
                            .map(String::from)
                            .unwrap_or_else(|| v.to_string())
                    })
                    .collect::<Vec<_>>()
                    .join("");
                format!("ipc.Client: response_code = {code}:\n{msg}")
            }
            Value::Object(map) => {
                if let Some(Value::String(e)) = map.get("error") {
                    e.clone()
                } else {
                    format!("ipc.Client: response_code = {code}:\n{result:#}")
                }
            }
            _ => format!("ipc.Client: response_code = {code}: {result:?}"),
        }
    }

    /// Recursively decode {"$tuple": [...]} into simple arrays.
    fn decode_qtile_tuples(val: Value) -> Value {
        match val {
            Value::Object(mut map) => {
                if map.len() == 1 && matches!(map.get("$tuple"), Some(Value::Array(_))) {
                    if let Some(Value::Array(arr)) = map.remove("$tuple") {
                        return Value::Array(
                            arr.into_iter().map(Self::decode_qtile_tuples).collect(),
                        );
                    }
                }
                Value::Object(
                    map.into_iter()
                        .map(|(k, v)| (k, Self::decode_qtile_tuples(v)))
                        .collect(),
                )
            }
            Value::Array(arr) => {
                Value::Array(arr.into_iter().map(Self::decode_qtile_tuples).collect())
            }
            _ => val,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_find_sockfile() {
        let cache = Some("/tmp/qtile-test-cache".to_string());

        let sock = find_sockfile_with_env(Some(":99".to_string()), cache.clone(), None, None);
        assert!(sock.to_str().unwrap().contains("qtilesocket.:99"));

        let sock_wl =
            find_sockfile_with_env(None, cache.clone(), Some("wayland-test".into()), None);
        assert!(sock_wl
            .to_str()
            .unwrap()
            .contains("qtilesocket.wayland-test"));

        let sock_x11 = find_sockfile_with_env(None, cache, None, Some(":1.0".into()));
        assert!(sock_x11.to_str().unwrap().contains("qtilesocket.:1.0"));
    }

    #[test]
    fn test_match_response_new_json_ipc() {
        let payload = json!({
            "message_type": "reply",
            "content": {
                "status": 0,
                "result": {"version": "0.1.dev"}
            }
        });
        let response = serde_json::to_string(&payload).unwrap();
        let result = Client::match_response(Ok(response), false)
            .expect("Should parse new JSON IPC reply with 'result'");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_json_ipc_data() {
        let payload = json!({
            "message_type": "reply",
            "content": {
                "status": 0,
                "data": {"version": "0.1.dev"}
            }
        });
        let response = serde_json::to_string(&payload).unwrap();
        let result = Client::match_response(Ok(response), false)
            .expect("Should parse new JSON IPC reply with 'data'");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_legacy() {
        let response = "[0, {\"version\": \"0.1.dev\"}]".to_string();
        let result =
            Client::match_response(Ok(response), false).expect("Should parse legacy IPC reply");
        assert_eq!(result, json!({"version": "0.1.dev"}));
    }

    #[test]
    fn test_match_response_flexible() {
        let legacy_response = "[0, \"ok\"]".to_string();
        let modern_response = json!({"status": 0, "result": "ok"}).to_string();
        assert_eq!(
            Client::match_response(Ok(legacy_response), false).unwrap(),
            "ok"
        );
        assert_eq!(
            Client::match_response(Ok(modern_response), false).unwrap(),
            "ok"
        );
    }

    #[test]
    fn test_match_response_errors() {
        assert!(Client::match_response(Ok("{invalid".into()), false).is_err());
        assert!(Client::match_response(Ok("[]".into()), false).is_err());

        let missing_status = json!({"result": "ok"}).to_string();
        assert!(Client::match_response(Ok(missing_status), false).is_err());

        let bad_status = json!({"status": "error", "result": "ok"}).to_string();
        assert!(Client::match_response(Ok(bad_status), false).is_err());

        let err_resp = json!({"status": 1, "result": "some error"}).to_string();
        let res = Client::match_response(Ok(err_resp), false);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("some error"));

        let weird_err = json!({"status": 1, "result": 123}).to_string();
        assert!(Client::match_response(Ok(weird_err), false).is_err());

        // Qtile "Session locked." style: plain {"error": "..."} object
        let locked = json!({"error": "Session locked."}).to_string();
        let res = Client::match_response(Ok(locked), false);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Session locked."));

        // Legacy array where result is {"error": "..."}
        let locked_legacy = json!([1, {"error": "Session locked."}]).to_string();
        let res = Client::match_response(Ok(locked_legacy), false);
        assert!(res.is_err());
        assert!(res.unwrap_err().to_string().contains("Session locked."));
    }

    #[test]
    fn test_decode_qtile_tuples() {
        let input = json!({
            "a": {"$tuple": [1, 2]},
            "b": [3, {"$tuple": [4, 5]}]
        });
        let expected = json!({
            "a": [1, 2],
            "b": [3, [4, 5]]
        });
        assert_eq!(Client::decode_qtile_tuples(input), expected);
    }

    #[test]
    fn test_decode_qtile_tuples_complex() {
        let input = json!({
            "a": {"$tuple": [{"$tuple": [1, 2]}, 3]},
            "b": [{"$tuple": [4, 5]}, {"c": {"$tuple": [6]}}]
        });
        let expected = json!({
            "a": [[1, 2], 3],
            "b": [[4, 5], {"c": [6]}]
        });
        assert_eq!(Client::decode_qtile_tuples(input), expected);
    }

    #[test]
    fn test_send_request_fail() {
        let path = Some(PathBuf::from("/nonexistent/path/qtile/qtilesocket.:0"));
        assert!(Client::connect_with_path(path).is_err());
    }

    #[test]
    #[ignore = "requires live Qtile socket"]
    fn test_client_connect_success() {
        assert!(Client::connect().is_ok());
    }

    #[test]
    fn test_client_connect_fail() {
        let path = Some(PathBuf::from("/nonexistent/path/qtile/qtilesocket.:0"));
        assert!(Client::connect_with_path(path).is_err());
    }

    #[test]
    fn test_find_sockfile_no_display_falls_back_to_wayland0() {
        // When display, wayland, and x11 are all None and no socket files exist,
        // the scanner exhausts all defaults and falls back to wayland-0.
        let sock = find_sockfile_with_env(
            None,
            Some("/tmp/qtile-test-no-socket-000".to_string()),
            None,
            None,
        );
        assert!(sock.to_str().unwrap().ends_with("qtilesocket.wayland-0"));
    }

    #[test]
    fn test_send_fails_without_socket() {
        // send() is a legacy wrapper; verify it propagates connect errors.
        let result = Client::send("/nonexistent".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn test_match_response_unexpected_message_type() {
        let payload = json!({
            "message_type": "error",
            "content": {"detail": "bad"}
        });
        let response = serde_json::to_string(&payload).unwrap();
        let result = Client::match_response(Ok(response), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("unexpected envelope message_type"));
    }

    #[test]
    fn test_match_response_legacy_single_element_ok() {
        // Array with status=0 and no second element → result is null.
        let result = Client::match_response(Ok("[0]".to_string()), false);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Null);
    }

    #[test]
    fn test_match_response_legacy_non_number_status() {
        let result = Client::match_response(Ok(r#"["bad_status"]"#.to_string()), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("status is not a number"));
    }

    #[test]
    fn test_match_response_unknown_top_level_type() {
        let result = Client::match_response(Ok("\"just_a_string\"".to_string()), false);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unknown top-level"));
    }

    #[test]
    fn test_match_response_error_result_array() {
        // format_error_result with an array of strings.
        let resp = json!({"status": 1, "result": ["error", " details"]}).to_string();
        let result = Client::match_response(Ok(resp), false);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("error details"), "got: {msg}");

        // format_error_result with an array containing a non-string element.
        let resp_mixed = json!({"status": 1, "result": ["msg", 42]}).to_string();
        let result_mixed = Client::match_response(Ok(resp_mixed), false);
        assert!(result_mixed.is_err());
        let msg_mixed = result_mixed.unwrap_err().to_string();
        assert!(msg_mixed.contains("msg42"), "got: {msg_mixed}");
    }

    #[test]
    fn test_match_response_error_result_object_no_error_key() {
        // format_error_result with an object that has no "error" key falls back to JSON.
        let resp = json!({"status": 1, "result": {"detail": "some info"}}).to_string();
        let result = Client::match_response(Ok(resp), false);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("detail"), "got: {msg}");
    }

    #[cfg(feature = "framing")]
    #[test]
    fn test_send_request_framed_success() {
        use std::io::{Read, Write};
        use std::os::unix::net::UnixListener;
        use std::thread;

        let tmp = std::env::temp_dir();
        let unique = std::process::id();
        let display_name = format!("framing-{unique}");
        // Discovery builds $XDG_CACHE_HOME/qtile/qtilesocket.<display>, so the
        // listener must be bound inside that exact path.
        let cache_dir = tmp.join(format!("qtile-frame-test-{unique}"));
        let qtile_dir = cache_dir.join("qtile");
        let socket_path = qtile_dir.join(format!("qtilesocket.{display_name}"));
        std::fs::create_dir_all(&qtile_dir).unwrap();

        let listener = UnixListener::bind(&socket_path).unwrap();

        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut header = [0u8; 4];
            stream.read_exact(&mut header).unwrap();
            let len = u32::from_be_bytes(header) as usize;
            let mut body = vec![0u8; len];
            stream.read_exact(&mut body).unwrap();
            // Reply with a minimal framed response
            let reply = r#"[0,"ok"]"#;
            let reply_bytes = reply.as_bytes();
            stream
                .write_all(&(reply_bytes.len() as u32).to_be_bytes())
                .unwrap();
            stream.write_all(reply_bytes).unwrap();
        });

        let prev_xdg = std::env::var("XDG_CACHE_HOME").ok();
        let prev_wayland = std::env::var("WAYLAND_DISPLAY").ok();
        std::env::set_var("XDG_CACHE_HOME", &cache_dir);
        std::env::set_var("WAYLAND_DISPLAY", &display_name);

        let result = Client::send_request(r#"[[], "status", [], {}, true]"#.to_string(), true);

        match prev_xdg {
            Some(v) => std::env::set_var("XDG_CACHE_HOME", v),
            None => std::env::remove_var("XDG_CACHE_HOME"),
        }
        match prev_wayland {
            Some(v) => std::env::set_var("WAYLAND_DISPLAY", v),
            None => std::env::remove_var("WAYLAND_DISPLAY"),
        }

        server.join().unwrap();
        std::fs::remove_dir_all(&cache_dir).ok();

        assert!(
            result.is_ok(),
            "Framed send_request should succeed: {:?}",
            result.err()
        );
        assert_eq!(result.unwrap(), r#"[0,"ok"]"#);
    }
}
